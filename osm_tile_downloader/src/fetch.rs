use std::path::Path;

use crate::config::*;
use anyhow::Context;
use anyhow::Result;

// lazy_static::lazy_static! {
//     pub static ref DB_FETCH_READY:
//          typed_sled::Tree::<FetchWorkItem, f64>
//           = typed_sled::Tree::<FetchWorkItem, f64>::open(
//             &SLED_DB, "fetch_ready_v3");

//         pub static ref DB_FETCH_DONE:
//             typed_sled::Tree::<FetchWorkItem, FetchWorkResult>
//              = typed_sled::Tree::<FetchWorkItem, FetchWorkResult>::open(
//                &SLED_DB, "fetch_done_v4");
// }

// pub fn fetch_queue_ready() -> Result<Vec<(FetchWorkItem, f64)>> {
//     let mut v = vec![];
//     for rez in DB_FETCH_READY.iter() {
//         v.push(rez?);
//     }
//     Ok(v)
// }

// pub fn fetch_queue_done() -> Result<Vec<(FetchWorkItem, FetchWorkResult)>> {
//     let mut v = vec![];
//     for rez in DB_FETCH_DONE.iter() {
//         v.push(rez?);
//     }
//     Ok(v)
// }

// #[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
// pub struct FetchWorkItem {
//     url: String,
//     path: PathBuf,
//     socks5_proxy: String,
// }

// #[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
// pub struct FetchWorkResult {
//     is_ok: bool,
//     err_txt:  String,
//     added_at: f64,
//     started_at: f64,
//     finished_at: f64,
// }

// pub async fn fetch_loop() -> () {
//     loop {
//         if fetch_iteration().await.is_err() {
//             eprintln!("fetch loop iteration failed!");
//         }
//         tokio::time::sleep(Duration::from_secs_f64(1.0)).await;
//     }
// }

// pub async fn fetch_loop() {
//     {
//         for k in  DB_FETCH_DONE.iter().map(|k| k.unwrap().0) {
//             DB_FETCH_DONE.remove(&k).unwrap();
//         }
//         for k in  DB_FETCH_READY.iter().map(|k| k.unwrap().0) {
//             DB_FETCH_READY.remove(&k).unwrap();
//         }
//     }

//     eprintln!("running fetcher loop.");
//     use futures::StreamExt;
//     futures::stream::iter(DB_FETCH_READY.watch_all())
//     .for_each_concurrent(LINKS_CONFIG.proxy_fetch_parallel as usize, |v| async move {
//         match v {
//             typed_sled::Event::Insert{ key: item, value: added_at } => {
//                 if worker_single_fetch(item.clone(), added_at).await.is_err() {
//                     eprintln!("failed to work single fetch.");
//                 };
//             },
//             typed_sled::Event::Remove {key: _ } => {}
//         }
//     }).await;
// }

// pub async fn _broken_queued_fetch(
//     url: &str,
//     path: &Path,
//     socks5_proxy: &str,
// ) -> Result<()> {
//     let item = FetchWorkItem {
//         url: url.to_owned(),
//         path:PathBuf::from(path),
//         socks5_proxy:socks5_proxy.to_owned(),
//     };

//     let mut subscriber = DB_FETCH_DONE.watch_prefix(&item);

//     DB_FETCH_READY.insert(&item, &get_current_timestamp())?;
//     // do_fetch(&item).await
//     while let Some(event) = (&mut subscriber).await {
//         if let Event::Insert { key: _, value: work_result } = event {
//             // assert!(item.eq(&item2));
//             DB_FETCH_DONE.remove(&item)?;
//             if work_result.is_ok {
//                 return Ok(())
//             } else {
//                 anyhow::bail!("fetch error: {}", work_result.err_txt)
//             }
//         }
//     }

//     anyhow::bail!("did not get back insert result event.")
// }

// async fn worker_single_fetch(item: FetchWorkItem, added_at: f64) -> Result<()> {
//     use typed_sled::transaction::Transactional;
//     let started_at = get_current_timestamp();
//     let res = do_fetch(&item).await;
//     let finished_at: f64 = get_current_timestamp();
//     let res = FetchWorkResult {
//         is_ok: res.is_ok(),
//         err_txt: if res.is_ok() {"".to_owned()} else {format!{"{}", res.unwrap_err()}},
//         added_at,
//         started_at,
//         finished_at
//     };

//     let tx: Result<(),  sled::transaction::TransactionError<()>> = (&*DB_FETCH_READY, &*DB_FETCH_DONE)
//     .transaction(move |(db_ready, db_done)| {
//             db_ready.remove(&item)?;
//             db_done.insert(&item, &res)?;
//             Ok::<(),  sled::transaction::ConflictableTransactionError<()>>(())
//     });
//     if tx.is_err() {
//         anyhow::bail!("tx error: {:?}", tx.err());
//     }
//     Ok(())
// }

pub async fn fetch_with_socks5(
    url: &str,
    path: &Path,
    socks5_proxy: &str,
) -> Result<()> {
    use rand::seq::SliceRandom;
    let user_agent = LINKS_CONFIG
        .user_agents
        .choose(&mut rand::thread_rng())
        .context("no user-agent")?;

    let mut curl_cmd = tokio::process::Command::new(LINKS_CONFIG.curl_path.clone());
    curl_cmd
        .arg("-s")
        // .arg("-L")
        // KV ARGS
        .arg("-o")
        .arg(path)
        .arg("--user-agent")
        .arg(user_agent)
        .arg("--socks5-hostname")
        .arg(socks5_proxy)
        .arg("--connect-timeout")
        .arg((LINKS_CONFIG.timeout_secs - 2).to_string())
        .arg("--max-time")
        .arg((LINKS_CONFIG.timeout_secs - 1).to_string())
        // URL
        .arg(url);
    // eprintln!("running curl proxy = {}; url = {}", socks5_proxy, url);
    let mut curl = curl_cmd.spawn()?;
    let curl_status = curl.wait().await?;
    if curl_status.success() {
        Ok(())
    } else {
        anyhow::bail!(
            "curl fail to get file using socks proxy = {:?}  url = {:?}",
            socks5_proxy,
            url
        )
    }
}
