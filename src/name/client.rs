pub struct Client {}

impl Client {
    pub fn new() -> Client {
        Client {}
    }

    pub fn registration_request(&self, question: String) -> Result<(), ()> {
        Ok(())
    }
}
