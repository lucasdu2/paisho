//! The environment parts of AgentDyn's suites (dailylife, github and shopping), from its
//! `default_suites/v1/tools/` clients whose names start with `dynamic_`, `file_client.py`,
//! `github_client.py` and `shopping_client.py`.
//!
//! These differ from AgentDojo's parts of the same kind, which its suites still use: the
//! email database holds an inbox for every address and delivers mail between them, the
//! filesystem is a tree with permissions, the bank holds every account, and web pages can
//! have forms that act when submitted.
//!
//! The suites share these parts but not every tool: dailylife's `send_money` pays from the
//! bank's current account, for example, and shopping's from the account linked to the shopper.
//! A tool that differs between suites goes on a view named for its behavior, and each suite
//! lists the views it uses (see [`crate::environments::combine_environments`]).

pub mod bank_database;
pub mod calendar;
pub mod email_database;
pub mod file_tree;
pub mod web_database;

/// Makes one-time passwords (OTPs), six digits each.
///
/// AgentDyn draws OTPs at random. Here they follow from a seed in the environment, so an
/// episode is determined by its starting state and a reference solution can know the OTP it
/// will be sent (see [`OtpGenerator::peek`]). The agent can't see the seed, only the emails.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub(crate) struct OtpGenerator {
    state: u64,
}

impl Default for OtpGenerator {
    fn default() -> Self {
        OtpGenerator {
            state: 0x0000_005E_ED0F_0A7E_u64,
        }
    }
}

impl OtpGenerator {
    /// The next OTP.
    pub(crate) fn next(&mut self) -> String {
        // SplitMix64.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        format!("{:06}", z % 1_000_000)
    }

    /// The OTP [`OtpGenerator::next`] will return, without advancing.
    pub(crate) fn peek(&self) -> String {
        self.clone().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otps_are_six_digits_and_repeatable() {
        let mut otps = OtpGenerator::default();
        let peeked = otps.peek();
        let first = otps.next();
        assert_eq!(peeked, first);
        assert_eq!(first.len(), 6);
        assert!(first.chars().all(|c| c.is_ascii_digit()));
        assert_ne!(otps.next(), first);
        assert_eq!(OtpGenerator::default().next(), first);
    }
}
