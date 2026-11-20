use std::{future::{Future, IntoFuture}, pin::Pin};

struct Request;
struct RequestExecution<T>(T);
trait Execute: Sized {
    fn execute(self) -> RequestExecution<Self> { RequestExecution(self) }
}
impl Execute for Request {}

impl<T> RequestExecution<T> {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async move { page().await })
    }
}
impl<T> IntoFuture for RequestExecution<T> {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = ()>>>;
    fn into_future(self) -> Self::IntoFuture { self.run() }
}
async fn page() {}
pub async fn entry() { Request {}.execute().await; }
