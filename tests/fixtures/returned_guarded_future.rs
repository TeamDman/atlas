use std::{future::{Future, IntoFuture}, pin::Pin};
struct Request;
struct Work<T>(T);
trait Execute: Sized { fn execute(self) -> Work<Self> { Work(self) } }
impl Execute for Request {}
trait Instrument: Sized { fn instrument(self, _: ()) -> Self { self } }
impl<T> Instrument for T {}
struct Scope;
impl Scope {
    fn guarded<F, Fut>(&self, factory: F) -> impl Future<Output = ()>
    where F: FnOnce(()) -> Fut, Fut: Future<Output = ()> {
        async move { if reject() { return; } let future = factory(()); if cancelled() { return; } future.await }
    }
}
fn reject() -> bool { false }
fn cancelled() -> bool { false }
fn marker() {}
impl<T> Work<T> {
    fn run(self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(Scope.guarded(move |child: ()| async move { let _ = child; marker(); }).instrument(()))
    }
}
impl<T> IntoFuture for Work<T> {
    type Output = ();
    type IntoFuture = Pin<Box<dyn Future<Output = ()>>>;
    fn into_future(self) -> Self::IntoFuture { self.run() }
}
pub async fn entry() { Request {}.execute().await; }
