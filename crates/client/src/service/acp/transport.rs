use super::super::pb;
use agent_client_protocol::{Client, ConnectTo, Lines};
use futures::{SinkExt, StreamExt};
use std::io;
use tonic::transport::Channel;

pub const CONNECTION_HEADER: &str = "bas-acp-connection";
pub type Control = pb::acp_transport_client::AcpTransportClient<Channel>;

pub struct Prepared {
    pub id: String,
    pub control: Control,
}

/// # Errors
/// Returns a connection or preparation failure without opening an ACP stream.
pub async fn prepare(
    channel: Channel,
    request: pb::PrepareAcpRequest,
) -> Result<Prepared, tonic::Status> {
    let mut control = Control::new(channel)
        .max_decoding_message_size(super::super::MAX_MESSAGE_SIZE)
        .max_encoding_message_size(super::super::MAX_MESSAGE_SIZE);
    let id = control.prepare(request).await?.into_inner().id;
    Ok(Prepared { id, control })
}

impl Prepared {
    /// # Errors
    /// Rejects an expired, already consumed or invalid prepared connection.
    pub async fn connect(&mut self) -> Result<Transport, tonic::Status> {
        let (outgoing, incoming) = futures::channel::mpsc::channel::<String>(32);
        let mut request = tonic::Request::new(incoming.map(|line| pb::AcpMessage {
            json: line.into_bytes(),
        }));
        request.metadata_mut().insert(
            CONNECTION_HEADER,
            self.id
                .parse()
                .map_err(|_| tonic::Status::invalid_argument("invalid connection ID"))?,
        );
        let incoming = self.control.open(request).await?.into_inner();
        Ok(Transport { outgoing, incoming })
    }
}

/// Usable directly with the standard ACP SDK's Client builder.
pub struct Transport {
    outgoing: futures::channel::mpsc::Sender<String>,
    incoming: tonic::Streaming<pb::AcpMessage>,
}
impl ConnectTo<Client> for Transport {
    async fn connect_to(
        self,
        client: impl ConnectTo<agent_client_protocol::Agent>,
    ) -> agent_client_protocol::Result<()> {
        let outgoing = self.outgoing.sink_map_err(io::Error::other);
        let incoming = self.incoming.map(|message| {
            let message = message.map_err(io::Error::other)?;
            String::from_utf8(message.json).map_err(io::Error::other)
        });
        ConnectTo::<Client>::connect_to(Lines::new(outgoing, incoming), client).await
    }
}
