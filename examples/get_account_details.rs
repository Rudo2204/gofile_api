use gofile::*;
use std::env::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();

    let api = Api::authorize(token);
    let account_details = api.get_account_details().await?;
    println!("{:?}", account_details);

    Ok(())
}

