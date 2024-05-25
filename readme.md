## A simplified crate of Spotify Auth Client

### Usage

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config =  &Configs::from_pass("", "");
    let mut handler = ClientHandler::new();
    let client = handler.client_new(config).await?;
    
    let track_id = TrackId::from_id("6D6Pybzey0shI8U9ttRAPx")?;
    let result = client.track(track_id, None).await?;

    dbg!(result);

    Ok(())
    }
```
