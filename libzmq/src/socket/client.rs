use crate::{addr::Endpoint, auth::*, core::*, error::*, Ctx, CtxHandle};

use serde::{Deserialize, Serialize};

use std::sync::Arc;

/// A `Client` socket is used for advanced request-reply messaging.
///
/// `Client` sockets are threadsafe and can be used from multiple threads at the
/// same time. Note that replies from a `Server` socket will go to the first
/// client thread that calls `recv`. If you need to get replies back to the
/// originating thread, use one `Client` socket per thread.
///
/// When a `Client` socket is connected to multiple sockets, outgoing
/// messages are distributed between connected peers on a round-robin basis.
/// Likewise, the `Client` socket receives messages fairly from each connected peer.
///
/// # Mute State
/// When `Client` socket enters the mute state due to having reached the high water
/// mark, or if there are no peers at all, then any send operations on the
/// socket shall block until the mute state ends or at least one peer becomes
/// available for sending; messages are not discarded.
///
/// # Summary of Characteristics
/// | Characteristic            | Value                  |
/// |:-------------------------:|:----------------------:|
/// | Compatible peer sockets   | [`Server`]             |
/// | Direction                 | Bidirectional          |
/// | Send/receive pattern      | Unrestricted           |
/// | Outgoing routing strategy | Round-robin            |
/// | Incoming routing strategy | Fair-queued            |
/// | Action in mute state      | Block                  |
///
/// # Example
/// ```
/// # use failure::Error;
/// #
/// # fn main() -> Result<(), Error> {
/// use libzmq::{prelude::*, *};
///
/// // Use a system assigned port.
/// let addr: TcpAddr = "127.0.0.1:*".try_into()?;
///
/// let server = ServerBuilder::new()
///     .bind(addr)
///     .build()?;
///
/// // Retrieve the addr that was assigned.
/// let bound = server.last_endpoint()?;
///
/// let client = ClientBuilder::new()
///     .connect(bound)
///     .build()?;
///
/// // Send a string request.
/// client.send("tell me something")?;
///
/// // Receive the client request.
/// let msg = server.recv_msg()?;
/// let id = msg.routing_id().unwrap();
///
/// // Reply to the client.
/// server.route("it takes 224 bits to store a i32 in java", id)?;
///
/// // We send as much replies as we want.
/// server.route("also don't talk to me", id)?;
///
/// // Retreive the first reply.
/// let mut msg = client.recv_msg()?;
/// // And the second.
/// client.recv(&mut msg)?;
/// #
/// #     Ok(())
/// # }
/// ```
///
/// [`Server`]: struct.Server.html
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    inner: Arc<RawSocket>,
}

impl Client {
    /// Create a `Client` socket from the [`global context`]
    ///
    /// # Returned Error Variants
    /// * [`InvalidCtx`]
    /// * [`SocketLimit`]
    ///
    /// [`InvalidCtx`]: enum.ErrorKind.html#variant.InvalidCtx
    /// [`SocketLimit`]: enum.ErrorKind.html#variant.SocketLimit
    /// [`global context`]: struct.Ctx.html#method.global
    pub fn new() -> Result<Self, Error> {
        let inner = Arc::new(RawSocket::new(RawSocketType::Client)?);

        Ok(Self { inner })
    }

    /// Create a `Client` socket associated with a specific context
    /// from a `CtxHandle`.
    ///
    /// # Returned Error Variants
    /// * [`InvalidCtx`]
    /// * [`SocketLimit`]
    ///
    /// [`InvalidCtx`]: enum.ErrorKind.html#variant.InvalidCtx
    /// [`SocketLimit`]: enum.ErrorKind.html#variant.SocketLimit
    pub fn with_ctx(handle: CtxHandle) -> Result<Self, Error> {
        let inner =
            Arc::new(RawSocket::with_ctx(RawSocketType::Client, handle)?);

        Ok(Self { inner })
    }

    /// Returns the handle to the `Ctx` of the socket.
    pub fn ctx(&self) -> CtxHandle {
        self.inner.ctx()
    }
}

impl GetRawSocket for Client {
    fn raw_socket(&self) -> &RawSocket {
        &self.inner
    }
}

impl Heartbeating for Client {}
impl Socket for Client {}
impl SendMsg for Client {}
impl RecvMsg for Client {}

unsafe impl Send for Client {}
unsafe impl Sync for Client {}

/// A configuration for a `Client`.
///
/// Especially helpfull in config files.
// We can't derive and use #[serde(flatten)] because of this issue:
// https://github.com/serde-rs/serde/issues/1346.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "FlatClientConfig")]
#[serde(from = "FlatClientConfig")]
pub struct ClientConfig {
    socket_config: SocketConfig,
    send_config: SendConfig,
    recv_config: RecvConfig,
    heartbeat_config: HeartbeatingConfig,
}

impl ClientConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(&self) -> Result<Client, Error> {
        self.with_ctx(Ctx::global())
    }

    pub fn with_ctx(&self, handle: CtxHandle) -> Result<Client, Error> {
        let client = Client::with_ctx(handle)?;
        self.apply(&client)?;

        Ok(client)
    }

    pub fn apply(&self, client: &Client) -> Result<(), Error> {
        self.send_config.apply(client)?;
        self.recv_config.apply(client)?;
        self.heartbeat_config.apply(client)?;
        self.socket_config.apply(client)?;

        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct FlatClientConfig {
    connect: Option<Vec<Endpoint>>,
    bind: Option<Vec<Endpoint>>,
    heartbeat: Option<Heartbeat>,
    send_hwm: HighWaterMark,
    send_timeout: Period,
    recv_hwm: HighWaterMark,
    recv_timeout: Period,
    mechanism: Option<Mechanism>,
}

impl From<ClientConfig> for FlatClientConfig {
    fn from(config: ClientConfig) -> Self {
        let socket_config = config.socket_config;
        let send_config = config.send_config;
        let recv_config = config.recv_config;
        let heartbeat_config = config.heartbeat_config;
        Self {
            connect: socket_config.connect,
            bind: socket_config.bind,
            heartbeat: heartbeat_config.heartbeat,
            mechanism: socket_config.mechanism,
            send_hwm: send_config.send_hwm,
            send_timeout: send_config.send_timeout,
            recv_hwm: recv_config.recv_hwm,
            recv_timeout: recv_config.recv_timeout,
        }
    }
}

impl From<FlatClientConfig> for ClientConfig {
    fn from(flat: FlatClientConfig) -> Self {
        let socket_config = SocketConfig {
            connect: flat.connect,
            bind: flat.bind,
            mechanism: flat.mechanism,
        };
        let send_config = SendConfig {
            send_hwm: flat.send_hwm,
            send_timeout: flat.send_timeout,
        };
        let recv_config = RecvConfig {
            recv_hwm: flat.recv_hwm,
            recv_timeout: flat.recv_timeout,
        };
        let heartbeat_config = HeartbeatingConfig {
            heartbeat: flat.heartbeat,
        };
        Self {
            socket_config,
            send_config,
            recv_config,
            heartbeat_config,
        }
    }
}

impl GetSocketConfig for ClientConfig {
    fn socket_config(&self) -> &SocketConfig {
        &self.socket_config
    }

    fn socket_config_mut(&mut self) -> &mut SocketConfig {
        &mut self.socket_config
    }
}

impl ConfigureSocket for ClientConfig {}

impl GetRecvConfig for ClientConfig {
    fn recv_config(&self) -> &RecvConfig {
        &self.recv_config
    }

    fn recv_config_mut(&mut self) -> &mut RecvConfig {
        &mut self.recv_config
    }
}

impl ConfigureRecv for ClientConfig {}

impl GetSendConfig for ClientConfig {
    fn send_config(&self) -> &SendConfig {
        &self.send_config
    }

    fn send_config_mut(&mut self) -> &mut SendConfig {
        &mut self.send_config
    }
}

impl ConfigureSend for ClientConfig {}

impl GetHeartbeatingConfig for ClientConfig {
    fn heartbeat_config(&self) -> &HeartbeatingConfig {
        &self.heartbeat_config
    }

    fn heartbeat_config_mut(&mut self) -> &mut HeartbeatingConfig {
        &mut self.heartbeat_config
    }
}

impl ConfigureHeartbeating for ClientConfig {}

/// A builder for a `Client`.
///
/// Allows for ergonomic one line socket configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientBuilder {
    inner: ClientConfig,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(&self) -> Result<Client, Error> {
        self.inner.build()
    }

    pub fn with_ctx(&self, handle: CtxHandle) -> Result<Client, Error> {
        self.inner.with_ctx(handle)
    }
}

impl GetSocketConfig for ClientBuilder {
    fn socket_config(&self) -> &SocketConfig {
        self.inner.socket_config()
    }

    fn socket_config_mut(&mut self) -> &mut SocketConfig {
        self.inner.socket_config_mut()
    }
}

impl BuildSocket for ClientBuilder {}

impl GetSendConfig for ClientBuilder {
    fn send_config(&self) -> &SendConfig {
        self.inner.send_config()
    }

    fn send_config_mut(&mut self) -> &mut SendConfig {
        self.inner.send_config_mut()
    }
}

impl BuildSend for ClientBuilder {}

impl GetRecvConfig for ClientBuilder {
    fn recv_config(&self) -> &RecvConfig {
        self.inner.recv_config()
    }

    fn recv_config_mut(&mut self) -> &mut RecvConfig {
        self.inner.recv_config_mut()
    }
}

impl BuildRecv for ClientBuilder {}

impl GetHeartbeatingConfig for ClientBuilder {
    fn heartbeat_config(&self) -> &HeartbeatingConfig {
        self.inner.heartbeat_config()
    }

    fn heartbeat_config_mut(&mut self) -> &mut HeartbeatingConfig {
        self.inner.heartbeat_config_mut()
    }
}

impl BuildHeartbeating for ClientBuilder {}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{prelude::TryInto, InprocAddr};

    #[test]
    fn test_ser_de() {
        let addr: InprocAddr = "test".try_into().unwrap();

        let mut config = ClientConfig::new();
        config.set_connect(Some(&addr));

        let ron = serde_yaml::to_string(&config).unwrap();
        let de: ClientConfig = serde_yaml::from_str(&ron).unwrap();
        assert_eq!(config, de);
    }
}
