use gofile_api::*;
use std::env::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = var("GOFILE_TOKEN").unwrap();

    let api = Api::default().authorize(token);
    let account_id = api.get_account_id().await?;
    let account_details = api.get_account_details(account_id).await?;
    let created_content = api
        .create_folder(account_details.root_folder, "001")
        .await?;
    println!("{:?}", created_content);

    Ok(())
}
