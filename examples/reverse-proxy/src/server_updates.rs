#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct Patch {
    pub html: String,
}
