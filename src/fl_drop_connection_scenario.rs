use crate::FlUrlResponse;

pub trait DropConnectionScenario {
    fn should_we_drop_it(&self, result: &FlUrlResponse) -> bool;
}

pub struct DefaultDropConnectionScenario;

impl DropConnectionScenario for DefaultDropConnectionScenario {
    fn should_we_drop_it(&self, result: &FlUrlResponse) -> bool {
        let status_code = result.get_status_code();

        if status_code > 400 || status_code == 499 {
            return status_code != 404;
        }

        false
    }
}
