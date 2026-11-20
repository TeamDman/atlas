use std::{future::{Future, IntoFuture}, pin::Pin};
struct Request;
struct Other;
struct Work<T> { request: T }
trait Execute: Sized {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>>;
    fn execute(self) -> Work<Self> { Work { request: self } }
}
impl Execute for Request {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async move { Helper::new().collect().await })
    }
}
impl Execute for Other { fn run(self) -> Pin<Box<dyn Future<Output = ()>>> { Box::pin(async {}) } }
struct Helper {}
impl Helper {
    fn new() -> Self { Self {} }
    fn collect(&self) -> impl Future<Output = ()> { self.collect_from() }
    async fn collect_from(&self) {
        let mut data = Vec::new();
        while let Some(value) = page().await {
            data.push(value);
            if done() { break; }
        }
    }
}
async fn page() -> Option<u8> { None }
fn done() -> bool { true }
trait Instrument: Sized { fn instrument(self, _: ()) -> Self { self } }
impl<T> Instrument for T {}
struct Scope;
impl Scope {
    fn outer<F, Fut>(&self, factory: F) -> impl Future<Output = ()>
    where F: FnOnce(()) -> Fut, Fut: Future<Output = ()> {
        async move { if rejected() { return; } let future = factory(()); if cancelled() { return; } future.await }
    }
    fn inner<F, Fut>(&self, factory: F) -> impl Future<Output = ()>
    where F: FnOnce(()) -> Fut, Fut: Future<Output = ()> { self.outer(factory) }
}
fn rejected() -> bool { false }
fn cancelled() -> bool { false }
impl<T: Execute + 'static> Work<T> {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(Scope.outer(move |child: ()| async move {
            let _ = child;
            Scope.inner(move |inner: ()| async move { let _ = inner; self.request.run().await }).await
        }).instrument(()))
    }
}
impl<T: Execute + 'static> IntoFuture for Work<T> {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = ()>>>;
    fn into_future(self) -> Self::IntoFuture { self.run() }
}
pub async fn entry() { Request {}.execute().await; }
