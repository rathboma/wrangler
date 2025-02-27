use crate::http;
use crate::settings::global_user::GlobalUser;
use crate::terminal::{emoji, message, styles};

use cloudflare::endpoints::account::{self, Account};
use cloudflare::endpoints::user::GetUserDetails;
use cloudflare::framework::apiclient::ApiClient;
use cloudflare::framework::response::ApiFailure;

use prettytable::{Cell, Row, Table};

pub fn whoami(user: &GlobalUser) -> Result<(), failure::Error> {
    let mut missing_permissions: Vec<String> = Vec::with_capacity(2);
    // Attempt to print email for both GlobalKeyAuth and TokenAuth users
    let auth: String = match user {
        GlobalUser::GlobalKeyAuth { email, .. } => {
            format!("a Global API Key, associated with the email '{}'", email,)
        }
        GlobalUser::TokenAuth { .. } => {
            let token_auth_email = fetch_api_token_email(user, &mut missing_permissions)?;

            if let Some(token_auth_email) = token_auth_email {
                format!(
                    "an API Token, associated with the email '{}'",
                    token_auth_email,
                )
            } else {
                "an API Token".to_string()
            }
        }
    };

    let accounts = fetch_accounts(user)?;
    let table = format_accounts(user, accounts, &mut missing_permissions);
    let mut msg = format!("{} You are logged in with {}!\n", emoji::WAVING, auth);
    let num_permissions_missing = missing_permissions.len();
    if num_permissions_missing > 0 {
        let login_msg = styles::highlight("`wrangler login`");
        let config_msg = styles::highlight("`wrangler config`");
        let whoami_msg = styles::highlight("`wrangler whoami`");
        if missing_permissions.len() == 1 {
            msg.push_str(&format!(
                "\nYour token is missing the '{}' permission.",
                styles::highlight(missing_permissions.get(0).unwrap())
            ));
        } else if missing_permissions.len() == 2 {
            msg.push_str(&format!(
                "\nYour token is missing the '{}' and '{}' permissions.",
                styles::highlight(missing_permissions.get(0).unwrap()),
                styles::highlight(missing_permissions.get(1).unwrap())
            ));
        }
        msg.push_str(&format!("\n\nPlease generate a new token and authenticate with {} or {}\nfor more information when running {}", login_msg, config_msg, whoami_msg));
    }
    message::billboard(&msg);
    if table.len() > 1 {
        println!("{}", &table);
    }
    Ok(())
}

fn fetch_api_token_email(
    user: &GlobalUser,
    missing_permissions: &mut Vec<String>,
) -> Result<Option<String>, failure::Error> {
    let client = http::cf_v4_client(user)?;
    let response = client.request(&GetUserDetails {});
    match response {
        Ok(res) => Ok(Some(res.result.email)),
        Err(e) => match e {
            ApiFailure::Error(_, api_errors) => {
                let error = &api_errors.errors[0];
                if error.code == 9109 {
                    missing_permissions.push("User Details: Read".to_string());
                }
                Ok(None)
            }
            ApiFailure::Invalid(_) => failure::bail!(http::format_error(e, None)),
        },
    }
}

fn fetch_accounts(user: &GlobalUser) -> Result<Vec<Account>, failure::Error> {
    let client = http::cf_v4_client(user)?;
    let response = client.request(&account::ListAccounts { params: None });
    match response {
        Ok(res) => Ok(res.result),
        Err(e) => failure::bail!(http::format_error(e, None)),
    }
}

fn format_accounts(
    user: &GlobalUser,
    accounts: Vec<Account>,
    missing_permissions: &mut Vec<String>,
) -> Table {
    let mut table = Table::new();
    let table_head = Row::new(vec![Cell::new("Account Name"), Cell::new("Account ID")]);
    table.add_row(table_head);

    if let GlobalUser::TokenAuth { .. } = user {
        if accounts.is_empty() {
            missing_permissions.push("Account Settings: Read".to_string());
        }
    }

    for account in accounts {
        let row = Row::new(vec![Cell::new(&account.name), Cell::new(&account.id)]);
        table.add_row(row);
    }
    table
}
