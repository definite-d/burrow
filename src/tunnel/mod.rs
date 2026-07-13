pub mod spawned;

use std::future::Future;
use std::pin::Pin;

pub trait Tunnel: Send {
    fn start(
        &mut self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + '_>>;
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}
