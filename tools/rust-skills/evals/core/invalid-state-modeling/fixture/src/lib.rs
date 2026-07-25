//! Settlement records for the internal billing tool.

/// One billing reference and how far its settlement has progressed.
pub struct Settlement {
    /// Account the settlement belongs to.
    pub account: String,
    /// Billing reference printed on the statement.
    pub reference: String,
    /// Amount owed, in dollars.
    pub amount_dollars: f64,
    /// Whether the settlement completed.
    pub settled: bool,
    /// Confirmation identifier, recorded once the settlement completes.
    pub confirmation: Option<String>,
    /// Failure description, recorded once the settlement fails.
    pub failure: Option<String>,
}

impl Settlement {
    /// Creates a settlement that has not been attempted yet.
    pub fn pending(account: String, reference: String, amount_dollars: f64) -> Self {
        Self {
            account,
            reference,
            amount_dollars,
            settled: false,
            confirmation: None,
            failure: None,
        }
    }

    /// Records a completed settlement.
    pub fn settle(&mut self, confirmation: String) {
        self.settled = true;
        self.confirmation = Some(confirmation);
    }

    /// Records a failed settlement attempt.
    pub fn fail(&mut self, failure: String) {
        self.failure = Some(failure);
    }
}

/// Renders the statement line for one settlement.
pub fn statement_line(settlement: &Settlement) -> String {
    if settlement.settled {
        match &settlement.confirmation {
            Some(confirmation) => format!("{} settled ({confirmation})", settlement.reference),
            None => format!("{} settled (confirmation missing)", settlement.reference),
        }
    } else if let Some(failure) = &settlement.failure {
        format!("{} failed: {failure}", settlement.reference)
    } else {
        format!("{} pending", settlement.reference)
    }
}

/// Totals the dollar amounts of the settlements that completed.
pub fn settled_total_dollars(settlements: &[Settlement]) -> f64 {
    settlements
        .iter()
        .filter(|settlement| settlement.settled)
        .map(|settlement| settlement.amount_dollars)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{Settlement, settled_total_dollars, statement_line};

    fn record(reference: &str, amount_dollars: f64) -> Settlement {
        Settlement::pending("ACCT-1".to_owned(), reference.to_owned(), amount_dollars)
    }

    #[test]
    fn renders_each_settlement_state() {
        let pending = record("INV-1", 10.50);
        assert_eq!(statement_line(&pending), "INV-1 pending");

        let mut settled = record("INV-2", 20.25);
        settled.settle("CONF-2".to_owned());
        assert_eq!(statement_line(&settled), "INV-2 settled (CONF-2)");

        let mut failed = record("INV-3", 5.00);
        failed.fail("card declined".to_owned());
        assert_eq!(statement_line(&failed), "INV-3 failed: card declined");
    }

    #[test]
    fn totals_only_settled_amounts() {
        let mut first = record("INV-1", 10.50);
        first.settle("CONF-1".to_owned());
        let mut second = record("INV-2", 20.25);
        second.settle("CONF-2".to_owned());
        let pending = record("INV-3", 99.99);

        assert_eq!(settled_total_dollars(&[first, second, pending]), 30.75);
    }
}
