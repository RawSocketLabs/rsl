use super::{MAX_FRAME_LEN, Stream};
use crate::error::Error;
use ::tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use bnb::{BitDecode, BitEncode};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

impl<S: AsyncRead + Unpin> Stream<S> {
    pub(crate) async fn read_message_async<T: BitDecode + BitEncode>(
        &mut self,
    ) -> Result<T, Error> {
        let mut bytes = [0; MAX_FRAME_LEN];
        bnb::net::read_message_async(&mut self.inner, &mut self.buffered, &mut bytes)
            .await
            .map_err(Error::from)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for Stream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if bytes.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let this = self.get_mut();
        let count = bytes.remaining().min(this.buffered_len()?);
        if count == 0 {
            Pin::new(&mut this.inner).poll_read(cx, bytes)
        } else {
            this.read_buffered(bytes.initialize_unfilled_to(count))?;
            bytes.advance(count);
            Poll::Ready(Ok(()))
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for Stream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, bytes)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
