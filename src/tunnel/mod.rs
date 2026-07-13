pub mod spawned;

use std::future::Future;
use std::pin::Pin;

pub trait Tunnel: Send + Sync {
    fn start(&self, port: u16) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
    fn stop(&self) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}
