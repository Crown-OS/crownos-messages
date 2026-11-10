//! The crownlink session: one connection, reconnected with backoff, carrying any number of
//! requests at once and turning crownlink's events into [`LinkEvent`]s.

use std::{ops::ControlFlow, path::PathBuf, time::Duration};

use crownlink_linux::ipc::proto::{self as ipc, DeviceInfo, FeatureState};
use crownos_ipc::{Client, EventMsg, MethodMsg, Pending, adapter::tokio::AsyncClient};
use serde::de::DeserializeOwned;
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until, timeout},
};

use crate::{convert, error::LinkError, events::LinkEvent};

const FIRST_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(15);
const SETUP_TIMEOUT: Duration = Duration::from_secs(5);

/// A request handed to the session, not yet written.
trait Exchange: Send {
    fn start(self: Box<Self>, client: &mut Client) -> Option<Box<dyn InFlight>>;
    fn refuse(self: Box<Self>, error: LinkError);
}

/// A request written to crownlink, waiting for its answer.
trait InFlight: Send {
    /// Delivers the answer if it came, or a timeout once `now` passes the deadline; `true`
    /// once crownlink is done with the request.
    fn poll(&mut self, client: &mut Client, now: Instant) -> bool;
    /// When the caller stops waiting; `None` once it has its answer or gave up.
    fn deadline(&self) -> Option<Instant>;
    fn fail(self: Box<Self>, error: LinkError);
}

type Reply<R> = oneshot::Sender<Result<R, LinkError>>;

struct Call<M: MethodMsg> {
    args: M::Args,
    patience: Duration,
    reply: Reply<M::Reply>,
}

impl<M> Exchange for Call<M>
where
    M: MethodMsg + 'static,
    M::Args: Send,
    M::Reply: Send,
{
    fn start(self: Box<Self>, client: &mut Client) -> Option<Box<dyn InFlight>> {
        match client.start_request::<M>(&self.args, Vec::new()) {
            Ok(pending) => Some(Box::new(Waiting {
                pending,
                reply: Some(self.reply),
                deadline: Instant::now() + self.patience,
            })),
            Err(error) => {
                self.reply.send(Err(error.into())).ok();
                None
            }
        }
    }

    fn refuse(self: Box<Self>, error: LinkError) {
        self.reply.send(Err(error)).ok();
    }
}

struct Waiting<R> {
    pending: Pending<R>,
    reply: Option<Reply<R>>,
    deadline: Instant,
}

impl<R: DeserializeOwned + Send> InFlight for Waiting<R> {
    fn poll(&mut self, client: &mut Client, now: Instant) -> bool {
        let answer = match client.take(&self.pending) {
            Ok(None) => {
                if now >= self.deadline
                    && let Some(reply) = self.reply.take()
                {
                    reply.send(Err(LinkError::Timeout)).ok();
                }
                return false;
            }
            Ok(Some(answer)) => Ok(answer),
            Err(error) => Err(error.into()),
        };
        if let Some(reply) = self.reply.take() {
            reply.send(answer).ok();
        }
        true
    }

    fn deadline(&self) -> Option<Instant> {
        self.reply.as_ref().map(|_| self.deadline)
    }

    fn fail(mut self: Box<Self>, error: LinkError) {
        if let Some(reply) = self.reply.take() {
            reply.send(Err(error)).ok();
        }
    }
}

/// Handle to the session; cheap to clone. Requests fail at once while crownlink is away.
#[derive(Debug, Clone)]
pub struct Link {
    exchanges: mpsc::UnboundedSender<Box<dyn Exchange>>,
}

impl Link {
    /// The handle, the session to run on a tokio runtime, and the events it reports. The
    /// session ends once every handle is dropped.
    #[must_use]
    pub fn new() -> (Self, Session, mpsc::UnboundedReceiver<LinkEvent>) {
        Self::with_endpoint(Endpoint::Session)
    }

    /// Like [`Link::new`] against a crownlink bound in `dir`.
    #[must_use]
    pub fn in_directory(
        dir: impl Into<PathBuf>,
    ) -> (Self, Session, mpsc::UnboundedReceiver<LinkEvent>) {
        Self::with_endpoint(Endpoint::Directory(dir.into()))
    }

    fn with_endpoint(endpoint: Endpoint) -> (Self, Session, mpsc::UnboundedReceiver<LinkEvent>) {
        let (exchanges, inbox) = mpsc::unbounded_channel();
        let (events, receiver) = mpsc::unbounded_channel();
        let session = Session {
            endpoint,
            inbox,
            events,
            devices: Vec::new(),
            available: None,
        };
        (Self { exchanges }, session, receiver)
    }

    /// Sends `args` and waits up to `patience` for crownlink's answer.
    pub(crate) async fn call<M>(
        &self,
        args: M::Args,
        patience: Duration,
    ) -> Result<M::Reply, LinkError>
    where
        M: MethodMsg + 'static,
        M::Args: Send,
        M::Reply: Send,
    {
        let (reply, answer) = oneshot::channel();
        let call: Box<dyn Exchange> = Box::new(Call::<M> {
            args,
            patience,
            reply,
        });
        self.exchanges
            .send(call)
            .map_err(|_| LinkError::Unavailable)?;
        answer.await.map_err(|_| LinkError::Unavailable)?
    }
}

#[derive(Debug, Clone)]
enum Endpoint {
    Session,
    Directory(PathBuf),
}

impl Endpoint {
    fn connect(&self) -> Result<AsyncClient, crownos_ipc::Error> {
        match self {
            Self::Session => AsyncClient::connect(ipc::SERVICE),
            Self::Directory(dir) => AsyncClient::connect_in(dir, ipc::SERVICE),
        }
    }
}

/// The task behind every [`Link`] handle.
pub struct Session {
    endpoint: Endpoint,
    inbox: mpsc::UnboundedReceiver<Box<dyn Exchange>>,
    events: mpsc::UnboundedSender<LinkEvent>,
    devices: Vec<DeviceInfo>,
    available: Option<bool>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Session")
            .field("endpoint", &self.endpoint)
            .field("available", &self.available)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Serves requests and follows crownlink until every [`Link`] handle is gone.
    pub async fn run(mut self) {
        let mut delay = FIRST_RETRY_DELAY;
        loop {
            if let Ok(client) = self.connect().await {
                delay = FIRST_RETRY_DELAY;
                if self.serve(client).await.is_break() {
                    return;
                }
            }
            self.mark_unavailable();
            if self.refuse_until(Instant::now() + delay).await.is_break() {
                return;
            }
            delay = (delay * 2).min(MAX_RETRY_DELAY);
        }
    }

    async fn connect(&mut self) -> Result<AsyncClient, LinkError> {
        let mut client = self.endpoint.connect()?;
        timeout(SETUP_TIMEOUT, subscribe_all(&mut client))
            .await
            .map_err(|_| LinkError::Timeout)??;
        self.refresh(&mut client).await?;
        Ok(client)
    }

    async fn serve(&mut self, mut client: AsyncClient) -> ControlFlow<()> {
        let mut in_flight: Vec<Box<dyn InFlight>> = Vec::new();
        loop {
            let deadline = in_flight
                .iter()
                .filter_map(|request| request.deadline())
                .min();
            let alive = tokio::select! {
                exchange = self.inbox.recv() => match exchange {
                    Some(exchange) => {
                        in_flight.extend(exchange.start(client.client_mut()));
                        true
                    }
                    None => return ControlFlow::Break(()),
                },
                inbound = client.pump() => inbound.is_ok(),
                () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => true,
            };
            let now = Instant::now();
            in_flight.retain_mut(|request| !request.poll(client.client_mut(), now));
            let in_sync = !self.drain(&mut client) || self.refresh(&mut client).await.is_ok();
            if !alive || !in_sync || client.client().is_closed() {
                in_flight
                    .into_iter()
                    .for_each(|request| request.fail(LinkError::Unavailable));
                return ControlFlow::Continue(());
            }
        }
    }

    async fn refresh(&mut self, client: &mut AsyncClient) -> Result<(), LinkError> {
        let call = client.call::<ipc::devices>(&ipc::devices {}, Vec::new());
        self.devices = timeout(SETUP_TIMEOUT, call)
            .await
            .map_err(|_| LinkError::Timeout)??;
        self.publish_devices();
        Ok(())
    }

    /// Applies the queued events; `true` when the device list must be fetched anew.
    fn drain(&mut self, client: &mut AsyncClient) -> bool {
        let mut changed = false;
        let mut stale = false;
        while let Some(message) = client.client_mut().next_event_raw() {
            let payload = &message.payload;
            let selector = message.selector;
            if selector == ipc::DevicesChanged::SELECTOR {
                if let Some(event) = decode::<ipc::DevicesChanged>(payload) {
                    self.devices = event.devices;
                    changed = true;
                }
            } else if selector == ipc::DeviceConnected::SELECTOR {
                if let Some(event) = decode::<ipc::DeviceConnected>(payload) {
                    match self.device_mut(event.id) {
                        Some(device) => {
                            device.connected = event.connected;
                            device.name = event.name;
                            changed = true;
                        }
                        None => stale = true,
                    }
                }
            } else if selector == ipc::BatteryChanged::SELECTOR {
                if let Some(event) = decode::<ipc::BatteryChanged>(payload)
                    && let Some(device) = self.device_mut(event.id)
                {
                    device.battery = Some(ipc::Battery {
                        percent: event.percent,
                        charging: event.charging,
                    });
                    changed = true;
                }
            } else if selector == ipc::FeatureChanged::SELECTOR {
                if let Some(event) = decode::<ipc::FeatureChanged>(payload) {
                    match self.device_mut(event.id) {
                        Some(device) => {
                            device.features = match event.state {
                                FeatureState::Disabled => device.features.without(event.feature),
                                FeatureState::Enabled | FeatureState::Active => {
                                    device.features.with(event.feature)
                                }
                            };
                            changed = true;
                        }
                        None => stale = true,
                    }
                }
            } else if selector == ipc::MessageChanged::SELECTOR {
                if let Some(event) = decode::<ipc::MessageChanged>(payload) {
                    self.emit(LinkEvent::MessageChanged {
                        device: convert::device_id_from(event.id),
                        change: convert::change(event.change),
                    });
                }
            } else if selector == ipc::CollectionsChanged::SELECTOR
                && let Some(event) = decode::<ipc::CollectionsChanged>(payload)
            {
                self.emit(LinkEvent::CollectionsChanged {
                    device: convert::device_id_from(event.id),
                    collections: convert::collections(event.collections),
                });
            }
        }
        if changed && !stale {
            self.publish_devices();
        }
        stale
    }

    fn device_mut(&mut self, id: ipc::DeviceId) -> Option<&mut DeviceInfo> {
        self.devices.iter_mut().find(|device| device.id == id)
    }

    fn publish_devices(&mut self) {
        self.available = Some(true);
        let paired = self
            .devices
            .iter()
            .filter(|device| device.trusted)
            .map(convert::device)
            .collect();
        self.emit(LinkEvent::Devices(paired));
    }

    fn emit(&self, event: LinkEvent) {
        self.events.send(event).ok();
    }

    fn mark_unavailable(&mut self) {
        if self.available == Some(false) {
            return;
        }
        self.available = Some(false);
        self.devices.clear();
        self.emit(LinkEvent::Unavailable);
    }

    /// Turns requests away until `deadline`; `Break` when every handle is gone.
    async fn refuse_until(&mut self, deadline: Instant) -> ControlFlow<()> {
        loop {
            tokio::select! {
                () = sleep_until(deadline) => return ControlFlow::Continue(()),
                exchange = self.inbox.recv() => match exchange {
                    Some(exchange) => exchange.refuse(LinkError::Unavailable),
                    None => return ControlFlow::Break(()),
                },
            }
        }
    }
}

async fn subscribe_all(client: &mut AsyncClient) -> Result<(), crownos_ipc::Error> {
    client.subscribe::<ipc::DevicesChanged>().await?;
    client.subscribe::<ipc::DeviceConnected>().await?;
    client.subscribe::<ipc::BatteryChanged>().await?;
    client.subscribe::<ipc::FeatureChanged>().await?;
    client.subscribe::<ipc::MessageChanged>().await?;
    client.subscribe::<ipc::CollectionsChanged>().await
}

fn decode<E: EventMsg + DeserializeOwned>(payload: &[u8]) -> Option<E> {
    crownos_ipc::postcard::from_bytes(payload).ok()
}
