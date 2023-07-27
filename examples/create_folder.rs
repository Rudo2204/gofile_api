use gofile::*;
use std::env::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();

    let api = Api::new().authorize(token);
    let account_details = api.get_account_details().await?;
    let created_content = api.create_folder(account_details.root_folder, "001").await?;
    println!("{:?}", created_content);

    Ok(())
}

