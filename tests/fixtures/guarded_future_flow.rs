use std::{future::{Future, IntoFuture}, pin::Pin};

struct Request;
struct RequestExecution<T>(T);
trait Execute: Sized {
    fn execute(self) -> RequestExecution<Self> { RequestExecution(self) }
}
impl Execute for Request {}

// A local stand-in for an instrumentation adapter, so this fixture requires no dependencies.
trait Instrument: Sized {
    fn instrument(self, _span: ()) -> Self { self }
}
impl<T> Instrument for T {}
fn admitted() -> bool { true }
fn cancelled_before_poll() -> bool { false }
fn guarded<F, Fut>(factory: F) -> impl Future<Output = ()>
where F: FnOnce() -> Fut, Fut: Future<Output = ()> {
    async move {
        if !admitted() { return; }
        let future = factory();
        if cancelled_before_poll() { return; }
        future.await
    }
}
impl<T> RequestExecution<T> {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(guarded(move || async move {
            guarded(move || async move { page().await }).await
        }).instrument(()))
    }
}
impl<T> IntoFuture for RequestExecution<T> {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = ()>>>;
    fn into_future(self) -> Self::IntoFuture { self.run() }
}
async fn page() {}
pub async fn entry() { Request {}.execute().await; }
