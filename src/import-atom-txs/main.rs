use backoff::backoff::Backoff;
use backoff::future::retry;
use backoff::{default, ExponentialBackoff};
use instant::Instant;
use serde::Deserialize;
use sqlx::PgPool;
use std::time::Duration;
use utils::db::connect;
use utils::memo::process_memo;
use std::{env, thread};

const PER_PAGE: &str = "100";

#[derive(Deserialize, Debug)]
struct ApiTxsResponse {
    tx: ApiTx,
    // body: body,
}

#[derive(Deserialize, Debug)]
struct body {
    messages: Vec<message>,
}

#[derive(Deserialize, Debug)]
struct message {
    from_address: String,
}

#[derive(Deserialize, Debug)]
struct ApiTx {
    body: ApiTxBody,
}

#[derive(Deserialize, Debug)]
struct ApiTxBody {
    messages: Vec<message>,
    memo: String,
}

#[derive(Deserialize, Debug)]
struct RpcResponse {
    jsonrpc: String,
    id: i32,
    result: RpcResult,
}

type RpcTxs = Vec<RpcTx>;

#[derive(Deserialize, Debug)]
struct RpcResult {
    txs: RpcTxs,
    total_count: String,
}

#[derive(Deserialize, Debug, Clone)]
struct RpcTx {
    pub hash: String,
    height: String,
}

async fn fetch_cosmos_raw_txs(page: u32) -> anyhow::Result<RpcResponse, reqwest::Error> {
    retry(ExponentialBackoff::default(), || async {
            let request = format!("http://localhost:26657/tx_search?query=\"transfer.recipient='{to_address}'\"&per_page={per_page}&page={page}",
                                  to_address = "cosmos1ejrf4cur2wy6kfurg9f2jppp2h3afe5h6pkh5t",
                                  per_page = PER_PAGE,
                                  page = page.to_string()
            );
            println!("request: {}", request);

            Ok(reqwest::get(&request).await?.json::<RpcResponse>().await?)
        })
        .await
}

async fn fetch_cosmos_account_txs(address: &str) -> anyhow::Result<ApiTxsResponse, reqwest::Error> {
    retry(ExponentialBackoff::default(), || async {
        let request = format!(
            "https://api.cosmostation.io/v1/account/txs/{to_address}",
            to_address = address
        );
        println!("request: {}", request);
        Ok(reqwest::get(&request)
            .await?
            .json::<ApiTxsResponse>()
            .await?)
    })
    .await
}

async fn fetch_cosmos_txs_details(hash: &str) -> anyhow::Result<ApiTxsResponse, reqwest::Error> {
    // retry(ExponentialBackoff::default(), || async {
        let request = format!(
            "http://localhost:1317/cosmos/tx/v1beta1/txs/{hash}",
            hash = hash
        );
        println!("request: {}", request);
        Ok(reqwest::get(&request)
            .await?
            .json::<ApiTxsResponse>()
            .await?)
    // })
    // .await
}

async fn process_cosmos_raw_txs(
    response: &RpcResponse,
    pool: &PgPool,
) -> anyhow::Result<(), reqwest::Error> {
    let txs = response.result.txs.clone();

    for tx in txs {
        // save network, hash and height to db

        match sqlx::query!(
            r#"
                        INSERT INTO txs (network, hash, height)
                        VALUES ( $1, $2, $3)
                        RETURNING id
                    "#,
            "ATOM",
            tx.hash,
            tx.height,
        )
        .fetch_one(pool)
        .await
        {
            Ok(_) => {
                let resp = fetch_cosmos_txs_details(&tx.hash).await?;
                // match fetch_cosmos_txs_details(&tx.hash).await {
                //     Ok(resp) => {
                        let memo = process_memo(&resp.tx.body.memo);
                        // save memo to db
                        match sqlx::query!(
                            r#"
                                UPDATE txs
                                SET memo = $1, twitter_handle = $5, sif_address = $6, from_address = $7
                                WHERE hash = $2 AND height = $3 AND network = $4
                                RETURNING id
                            "#,
                            resp.tx.body.memo,
                            tx.hash,
                            tx.height,
                            "ATOM",
                            memo.handle,
                            memo.address,
                            resp.tx.body.messages.get(0).unwrap().from_address
                        )
                        .fetch_one(pool)
                        .await
                        {
                            Ok(_) => println!("record saved"),
                            Err(e) => println!("error: {}", e),
                        }
                thread::sleep(Duration::from_millis(350));

                //     }
                //     Err(e) => return Err(e),
                // };
            }
            Err(_) => println!("already saved: {}", tx.hash),
        }
    }
    Ok(())
}

async fn process_incoming_cosmos_txs(pool: &PgPool) -> anyhow::Result<()> {
    let mut count = 0u32;

    // Get raw txs data
    loop {
        count += 1;
        println!("count: {}", count);

        let response = fetch_cosmos_raw_txs(count).await?;
        process_cosmos_raw_txs(&response, &pool).await?;

        let total: u32 = response.result.total_count.parse().unwrap();
        println!("total_count: {}", total);

        if count == total / 100 + 1 {
            println!("Finished. Break loop");
            break;
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = connect().await?;

    process_incoming_cosmos_txs(&pool).await?;

    Ok(())
}
