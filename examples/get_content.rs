use gofile::*;
use std::env::*;
use url::*;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let url = Url::parse(&args().collect::<Vec<_>>()[1]).unwrap();
    let token = var("GOFILE_TOKEN").unwrap();

    let api = Api::new().authorize(token);
    let content = api.get_content(&url).await?;
    println!("{:?}", content);

    Ok(())
}

