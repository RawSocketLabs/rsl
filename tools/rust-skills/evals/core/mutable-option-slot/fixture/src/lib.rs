pub fn take_name(slot: &mut Option<String>) -> String {
    slot.take().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::take_name;

    #[test]
    fn takes_the_name_and_clears_the_slot() {
        let mut slot = Some("receiver".to_owned());

        assert_eq!(take_name(&mut slot), "receiver");
        assert_eq!(slot, None);
    }
}
