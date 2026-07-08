use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use dns_types::HandleError;
use dns_types::zone::Zones;
use dns_types::zone::alias::AliasCache;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConnectionError {
    #[error("connection I/O failed: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Handle(#[from] HandleError),
}

pub(crate) async fn handle_connection(
    mut stream: TcpStream,
    zones: Arc<Zones>,
    aliases: Arc<AliasCache>,
    idle_timeout: Duration,
    shutdown: CancellationToken,
) -> Result<(), ConnectionError> {
    let _ = stream.set_nodelay(true);

    loop {
        let len = tokio::select! {
            biased;
            _ = shutdown.cancelled() => return Ok(()),
            read = tokio::time::timeout(idle_timeout, stream.read_u16()) => match read {
                Err(_elapsed) => return Ok(()),
                Ok(Err(error)) if error.kind() == ErrorKind::UnexpectedEof => return Ok(()),
                Ok(Err(error)) => return Err(error.into()),
                Ok(Ok(len)) => len as usize,
            },
        };

        if len == 0 {
            return Ok(());
        }

        let mut query = vec![0u8; len];
        stream.read_exact(&mut query).await?;

        let response = crate::answer::answer("tcp", &query, &zones, &aliases)?.bytes;

        let mut framed = Vec::with_capacity(2 + response.len());
        framed.extend_from_slice(&(response.len() as u16).to_be_bytes());
        framed.extend_from_slice(&response);

        stream.write_all(&framed).await?;
        stream.flush().await?;
    }
}
