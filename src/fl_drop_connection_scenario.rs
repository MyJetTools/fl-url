/// When a connection is closed after its response instead of going back to the pool:
/// a status above 400 other than 404. The rule is fixed — fl-url takes no rule of the
/// caller's.
pub(crate) fn should_drop_connection_by_status(status_code: u16) -> bool {
    if status_code > 400 || status_code == 499 {
        return status_code != 404;
    }

    false
}
