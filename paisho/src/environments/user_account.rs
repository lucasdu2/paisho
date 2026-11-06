//! AgentDojo's user account (`default_suites/v1/tools/user_account.py`).

use paisho_macros::tools;

/// Represents a simple user account.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserAccount {
    /// First name of the user
    pub(crate) first_name: String,
    /// Last name of the user
    pub(crate) last_name: String,
    /// Street of the user
    pub(crate) street: String,
    /// City of the user
    pub(crate) city: String,
    /// Password of the user
    pub(crate) password: String,
}

/// The user's information, without the password.
#[derive(Debug, Clone, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub(crate) struct UserInfo {
    pub(crate) first_name: String,
    pub(crate) last_name: String,
    pub(crate) street: String,
    pub(crate) city: String,
}

impl UserAccount {
    fn info(&self) -> UserInfo {
        UserInfo {
            first_name: self.first_name.clone(),
            last_name: self.last_name.clone(),
            street: self.street.clone(),
            city: self.city.clone(),
        }
    }
}

#[tools]
impl UserAccount {
    /// Get the user information.
    fn get_user_info(&self) -> Result<UserInfo, String> {
        Ok(self.info())
    }

    /// Update the user password.
    fn update_password(
        &mut self,
        #[describe = "New password for the user"] password: &str,
    ) -> Result<String, String> {
        self.password = password.to_string();
        Ok("Password updated.".to_string())
    }

    /// Update the user information.
    ///
    /// Each field given is set. AgentDojo skips empty strings; here they are set like any
    /// other value.
    fn update_user_info(
        &mut self,
        #[describe = "First name of the user (optional)"] first_name: Option<String>,
        #[describe = "Last name of the user (optional)"] last_name: Option<String>,
        #[describe = "Street of the user (optional)"] street: Option<String>,
        #[describe = "City of the user (optional)"] city: Option<String>,
    ) -> Result<UserInfo, String> {
        if let Some(first_name) = first_name {
            self.first_name = first_name;
        }
        if let Some(last_name) = last_name {
            self.last_name = last_name;
        }
        if let Some(street) = street {
            self.street = street;
        }
        if let Some(city) = city {
            self.city = city;
        }
        Ok(self.info())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::suites::call;

    #[test]
    fn info_leaves_out_the_password_and_updates_set_only_the_fields_given() {
        let mut account = UserAccount {
            first_name: "Emma".to_string(),
            last_name: "Johnson".to_string(),
            street: "Apple Street 1".to_string(),
            city: "Cupertino".to_string(),
            password: "password123".to_string(),
        };
        let update = call("update_user_info", json!({ "city": "New York" }));
        let result = account.dispatch(&update).unwrap();
        assert_eq!(
            result.structured_content.unwrap()["result"],
            json!({
                "first_name": "Emma", "last_name": "Johnson",
                "street": "Apple Street 1", "city": "New York",
            })
        );
    }
}
