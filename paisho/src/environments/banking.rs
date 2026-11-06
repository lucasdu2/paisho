//! AgentDojo's bank account (`default_suites/v1/tools/banking_client.py`).

use paisho_macros::tools;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Transaction {
    /// ID of the transaction
    pub(crate) id: u32,
    /// IBAN of the sender
    pub(crate) sender: String,
    /// IBAN of the recipient
    pub(crate) recipient: String,
    /// Amount of the transaction
    pub(crate) amount: f64,
    /// Subject of the transaction
    pub(crate) subject: String,
    /// Date of the transaction
    pub(crate) date: String,
    /// Is the transaction recurring
    pub(crate) recurring: bool,
}

/// The user's bank account. As in AgentDojo, sending money doesn't change the balance.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BankAccount {
    pub(crate) balance: f64,
    pub(crate) iban: String,
    pub(crate) transactions: Vec<Transaction>,
    pub(crate) scheduled_transactions: Vec<Transaction>,
}

/// The default for `get_most_recent_transactions`' `n`.
fn hundred() -> usize {
    100
}

impl BankAccount {
    /// One more than the largest ID among all transactions, scheduled or not, or 1 if there
    /// are none.
    pub(crate) fn next_id(&self) -> u32 {
        self.transactions
            .iter()
            .chain(&self.scheduled_transactions)
            .map(|t| t.id)
            .max()
            .unwrap_or(0)
            + 1
    }

    /// A new transaction from this account.
    fn transaction(
        &self,
        recipient: &str,
        amount: f64,
        subject: &str,
        date: &str,
        recurring: bool,
    ) -> Transaction {
        Transaction {
            id: self.next_id(),
            sender: self.iban.clone(),
            recipient: recipient.to_string(),
            amount,
            subject: subject.to_string(),
            date: date.to_string(),
            recurring,
        }
    }
}

#[tools]
impl BankAccount {
    /// Get the IBAN of the current bank account.
    fn get_iban(&self) -> Result<String, String> {
        Ok(self.iban.clone())
    }

    /// Sends a transaction to the recipient.
    fn send_money(
        &mut self,
        #[describe = "IBAN of the recipient"] recipient: &str,
        #[describe = "Amount of the transaction"] amount: f64,
        #[describe = "Subject of the transaction"] subject: &str,
        #[describe = "Date of the transaction"] date: &str,
    ) -> Result<String, String> {
        let transaction = self.transaction(recipient, amount, subject, date, false);
        self.transactions.push(transaction);
        Ok(format!("Transaction to {recipient} for {amount} sent."))
    }

    /// Schedule a transaction.
    fn schedule_transaction(
        &mut self,
        #[describe = "IBAN of the recipient"] recipient: &str,
        #[describe = "Amount of the transaction"] amount: f64,
        #[describe = "Subject of the transaction"] subject: &str,
        #[describe = "Next date of the transaction"] date: &str,
        #[describe = "Is the transaction recurring"] recurring: bool,
    ) -> Result<String, String> {
        let transaction = self.transaction(recipient, amount, subject, date, recurring);
        self.scheduled_transactions.push(transaction);
        Ok(format!(
            "Transaction to {recipient} for {amount} scheduled."
        ))
    }

    /// Update a scheduled transaction.
    ///
    /// Each field given is set. AgentDojo skips falsy values, so there `recurring: false`
    /// and `amount: 0` do nothing; here they are set like any other value.
    fn update_scheduled_transaction(
        &mut self,
        #[describe = "ID of the transaction (mandatory)"] id: u32,
        #[describe = "IBAN of the recipient (optional)"] recipient: Option<String>,
        #[describe = "Amount of the transaction (optional)"] amount: Option<f64>,
        #[describe = "Subject of the transaction (optional)"] subject: Option<String>,
        #[describe = "Next date of the transaction (optional)"] date: Option<String>,
        #[describe = "Is the transaction recurring (optional)"] recurring: Option<bool>,
    ) -> Result<String, String> {
        let transaction = self
            .scheduled_transactions
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("Transaction with ID {id} not found."))?;
        if let Some(recipient) = recipient {
            transaction.recipient = recipient;
        }
        if let Some(amount) = amount {
            transaction.amount = amount;
        }
        if let Some(subject) = subject {
            transaction.subject = subject;
        }
        if let Some(date) = date {
            transaction.date = date;
        }
        if let Some(recurring) = recurring {
            transaction.recurring = recurring;
        }
        Ok(format!("Transaction with ID {id} updated."))
    }

    /// Get the balance of the account.
    fn get_balance(&self) -> Result<f64, String> {
        Ok(self.balance)
    }

    /// Get the list of the most recent transactions, e.g. to summarize the last n
    /// transactions.
    fn get_most_recent_transactions(
        &self,
        #[describe = "Number of transactions to return"]
        #[serde(default = "hundred")]
        n: usize,
    ) -> Result<&[Transaction], String> {
        let start = self.transactions.len().saturating_sub(n);
        Ok(&self.transactions[start..])
    }

    /// Get the list of scheduled transactions.
    fn get_scheduled_transactions(&self) -> Result<&[Transaction], String> {
        Ok(&self.scheduled_transactions)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::suites::{ToolResult, call};

    fn account() -> BankAccount {
        serde_json::from_value(json!({
            "balance": 100.0,
            "iban": "DE00",
            "transactions": [
                { "id": 1, "sender": "me", "recipient": "A", "amount": 1.0, "subject": "a",
                  "date": "2022-01-01", "recurring": false },
                { "id": 2, "sender": "me", "recipient": "B", "amount": 2.0, "subject": "b",
                  "date": "2022-01-02", "recurring": false },
            ],
            "scheduled_transactions": [
                { "id": 5, "sender": "DE00", "recipient": "C", "amount": 3.0, "subject": "c",
                  "date": "2022-02-01", "recurring": true },
            ],
        }))
        .unwrap()
    }

    fn run(account: &mut BankAccount, name: &str, arguments: Value) -> ToolResult {
        account.dispatch(&call(name, arguments)).unwrap()
    }

    #[test]
    fn new_transactions_take_the_next_id_across_both_lists() {
        let mut account = account();
        let sent = json!({ "recipient": "X", "amount": 4.5, "subject": "s", "date": "d" });
        assert!(!run(&mut account, "send_money", sent).is_error);
        let sent = &account.transactions[2];
        assert_eq!(
            (sent.id, sent.sender.as_str(), sent.recurring),
            (6, "DE00", false)
        );
        assert_eq!(account.balance, 100.0);
    }

    #[test]
    fn most_recent_transactions_are_the_last_n() {
        let mut account = account();
        let result = run(
            &mut account,
            "get_most_recent_transactions",
            json!({ "n": 1 }),
        );
        assert_eq!(result.structured_content.unwrap()["result"][0]["id"], 2);
        let result = run(&mut account, "get_most_recent_transactions", json!({}));
        assert_eq!(
            result.structured_content.unwrap()["result"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn updates_set_only_the_fields_given() {
        let mut account = account();
        let update = json!({ "id": 5, "amount": 7.0, "recurring": false });
        assert!(!run(&mut account, "update_scheduled_transaction", update).is_error);
        let updated = &account.scheduled_transactions[0];
        assert_eq!((updated.amount, updated.recurring), (7.0, false));
        assert_eq!(updated.recipient, "C");

        let missing = run(
            &mut account,
            "update_scheduled_transaction",
            json!({ "id": 9 }),
        );
        assert!(missing.is_error);
    }
}
