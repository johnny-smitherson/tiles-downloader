use std::time::Duration;
use std::{path::Path};

use crate::config::{self, *};
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};

lazy_static::lazy_static! {
    pub static ref DB_SCRAPER_LAST_REFRESH:  typed_sled::Tree::<String, f64> = typed_sled::Tree::<String, f64>::open(&SLED_DB, "socks5_scraper_last_refresh_f64");
    pub static ref DB_SOCKS5_PROXY_ENTRY:  typed_sled::Tree::<String, Socks5ProxyEntry> = typed_sled::Tree::<String, Socks5ProxyEntry>::open(&SLED_DB, "socks5_proxy_entry_v2");
}
const SCRAPER_REFRESH_SECONDS: f64 = 300.0;
const ENTRY_DELETE_SECONDS: f64 = 1600.0;
use crate::config::get_current_timestamp;

#[derive(Deserialize, Clone, Debug, Serialize, PartialEq)]
pub struct Socks5ProxyEntry {
    pub addr: String,
    pub category: String,
    pub last_check: Option<f64>,
    pub last_lag: Option<f64>,
    pub last_scraped: f64,
    pub last_check_error: String,
    pub last_remote_ip: String,
    pub checked: bool,
    pub accepted: bool,

    pub created_at: f64,
    pub failed_checks: u8,
    pub last_success_count: u64,
    pub last_err_count: u64,
}

impl Socks5ProxyEntry {
    fn needs_recheck(&self) -> bool {
        (!self.checked)
            || (self.last_check.unwrap_or(0.0) + SCRAPER_REFRESH_SECONDS * ((self.failed_checks as f64) * 0.3 + 1.0)
                < get_current_timestamp())
    }
    fn needs_delete(&self) -> bool {
        (self.checked)
            && (!self.accepted)
            && (get_current_timestamp() - self.last_scraped > ENTRY_DELETE_SECONDS)
    }
}


async fn download_once_tor(url: &str, path: &Path) -> Result<()> {
    use rand::seq::SliceRandom;
    let socks5_proxy = LINKS_CONFIG
        .tor_addr_list
        .choose(&mut rand::thread_rng())
        .context("no socks proxy")?;
    crate::fetch::fetch(url, path, socks5_proxy).await
}

async fn download_socks5_proxy_list(
    proxy_scraper: &Socks5ProxyScraperConfig,
) -> Result<std::path::PathBuf> {
    let dir_path = LINKS_CONFIG.tile_location.join("socks5").join("lists");
    tokio::fs::create_dir_all(&dir_path).await?;
    let temp_file = tempfile().await?;
    let path = dir_path.join(format!(
        "{}.{}",
        proxy_scraper.name, proxy_scraper.extract_method
    ));

    download_once_tor(&proxy_scraper.url, temp_file.file_path()).await?;
    tokio::fs::rename(temp_file.file_path(), &path).await?;

    Ok(path)
}

async fn parse_socks5_proxy_list(path: &Path) -> anyhow::Result<Vec<String>> {
    let allowed_bytes: &[u8; 11] = b"1234567890.";
    let replace_byte: u8 = b" "[0];
    let re: regex::Regex =
        regex::Regex::new(r"(\d{1,3}).(\d{1,3}).(\d{1,3}).(\d{1,3}) (\d{2,5})")
            .unwrap();

    let text = tokio::fs::read(&path).await?;
    let text: Vec<u8> = text
        .iter()
        .map(|_c| {
            if allowed_bytes.contains(_c) {
                *_c
            } else {
                replace_byte
            }
        })
        .collect();
    let mut text: String = String::from_utf8_lossy(text.as_slice()).to_string();
    for _ in 0..=5 {
        text = text.replacen("    ", " ", 1000);
        text = text.replacen("  ", " ", 1000);
    }
    let text = text;
    let mut found_socks = Vec::<String>::new();
    for cap in re.captures_iter(text.as_str()) {
        let a: i32 = cap[1].parse().unwrap();
        let b: i32 = cap[2].parse().unwrap();
        let c: i32 = cap[3].parse().unwrap();
        let d: i32 = cap[4].parse().unwrap();
        let port: u32 = cap[5].parse().unwrap();
        let cond = [
            a >= 1,
            b >= 1,
            c >= 1,
            d >= 1,
            a <= 255,
            b <= 255,
            c <= 255,
            d <= 255,
            port >= 80,
            port <= 65536,
        ];
        if !cond.contains(&false) {
            let new_socks = format!(
                "{}.{}.{}.{}:{}",
                &cap[1], &cap[2], &cap[3], &cap[4], &cap[5]
            );
            found_socks.push(new_socks);
        }
    }
    Ok(found_socks)
}

async fn refresh_single_socks5_proxy_list(
    srv: &Socks5ProxyScraperConfig,
) -> anyhow::Result<()> {
    let old_last_hit = DB_SCRAPER_LAST_REFRESH.get(&srv.name);
    let should_refresh = {
        if let Ok(Some(last_ts)) = old_last_hit {
            last_ts + SCRAPER_REFRESH_SECONDS < get_current_timestamp()
        } else {
            true
        }
    };
    if !should_refresh {
        anyhow::bail!("too soon for refresh: {}", srv.name);
    }
    DB_SCRAPER_LAST_REFRESH.insert(&srv.name, &get_current_timestamp())?;
    let path = download_socks5_proxy_list(srv).await?;
    let found_socks = parse_socks5_proxy_list(&path).await?;
    if found_socks.is_empty() {
        anyhow::bail!("no proxy found for {}", srv.name);
    }
    let mut new_addr_count: u64 = 0;
    let mut existing_addr_count: u64 = 0;
    for addr in found_socks {
        if let Ok(Some(mut existing_item)) = DB_SOCKS5_PROXY_ENTRY.get(&addr) {
            existing_addr_count += 1;
            existing_item.last_scraped = get_current_timestamp();
            existing_item.category = srv.name.clone();
            DB_SOCKS5_PROXY_ENTRY.insert(&existing_item.addr, &existing_item)?;
        } else {
            DB_SOCKS5_PROXY_ENTRY.insert(
                &addr,
                &Socks5ProxyEntry {
                    addr: addr.clone(),
                    category: srv.name.clone(),
                    last_check: None,
                    last_lag: None,
                    checked: false,
                    accepted: false,
                    last_scraped: get_current_timestamp(),
                    last_check_error: "".to_owned(),
                    last_remote_ip: "".to_owned(),
                    created_at: get_current_timestamp(),
                    failed_checks: 0,
                    last_success_count: 0,
                    last_err_count: 0,
                },
            )?;
            new_addr_count += 1;
        }
    }
    eprintln!(
        "proxy refresh for {}: found {} new addr, {} old addr",
        srv.name, new_addr_count, existing_addr_count
    );
    Ok(())
}

async fn refresh_all_socks5_proxy_lists() -> anyhow::Result<()> {
    for srv in get_all_socks5_scrapers()? {
        let _refreshed = refresh_single_socks5_proxy_list(&srv).await;
        if _refreshed.is_err() {
            eprintln!(
                "failed to refresh socks5 list from {}: {}",
                srv.name,
                _refreshed.err().unwrap()
            )
        }
    }
    Ok(())
}

async fn _socks5_check_proxy(proxy: &mut Socks5ProxyEntry) -> anyhow::Result<()> {
    let temp_file = tempfile().await?;
    crate::fetch::fetch(
        "http://icanhazip.com/",
        temp_file.file_path(),
        &proxy.addr,
    )
    .await?;
    let resp = String::from_utf8_lossy(
        tokio::fs::read(temp_file.file_path()).await?.as_slice(),
    )
    .to_string();

    fn truncate(s: &str, max_chars: usize) -> &str {
        match s.char_indices().nth(max_chars) {
            None => s,
            Some((idx, _)) => &s[..idx],
        }
    }
    let resp = truncate(&resp, 41).trim();

    let is_ipv4 = resp.parse::<std::net::Ipv4Addr>().is_ok();
    let is_ipv6 = resp.parse::<std::net::Ipv6Addr>().is_ok();
    proxy.last_remote_ip = resp.to_owned();
    if is_ipv4 || is_ipv6 || resp.eq("阻断未备案") {
        Ok(())
    } else {
        anyhow::bail!("bad ip address from icanhazip: '{}'", resp)
    }
}

#[allow(unused_assignments)]
pub async fn proxy_manager_iteration() -> Result<()> {
    use futures::StreamExt;

    refresh_all_socks5_proxy_lists().await?;

    eprintln!("proxy check begin");
    let mut _deleted = 0;

    let all_proxies = get_all_proxy_entries();
    let addr_list: Vec<&str> = all_proxies.iter().map(|e| e.addr.as_str()).collect();
    let all_proxy_event_count = &config::stat_count_events_for_items(&addr_list);
    futures::stream::iter(all_proxies)
        .for_each_concurrent(LINKS_CONFIG.proxy_fetch_parallel as usize, |mut v| async move {
            // check proxy event count and save
            if let Some(x) = all_proxy_event_count.get(&v.addr) {
                v.last_success_count = *x.get("success").unwrap_or(&0);
                v.last_err_count = *x.get("fail").unwrap_or(&0);
                if (v.last_success_count != 0 || v.last_err_count != 0) && DB_SOCKS5_PROXY_ENTRY.insert(&v.addr, &v).is_err() {
                    eprintln!("db failed to overwrite socks5 item: {}", &v.addr);
                }
            }

            if !v.needs_recheck() {
                return;
            }
            let t0 = get_current_timestamp();
            let check = _socks5_check_proxy(&mut v).await;
            v.last_check = Some(get_current_timestamp());
            v.checked = true;
            v.accepted = check.is_ok();
            if v.accepted {
                v.failed_checks = 0;
            } else {
                v.failed_checks += 1;
            }
            v.last_lag = Some(get_current_timestamp() - t0);
            v.last_check_error = if check.is_ok() {
                "".to_owned()
            } else {
                format!("check err: {:?}", check.err())
            };

            if DB_SOCKS5_PROXY_ENTRY.insert(&v.addr, &v).is_err() {
                eprintln!("db failed to overwrite socks5 item: {}", &v.addr);
            }
            // do the delete last, to keep some older entries after reboot
            if v.needs_delete() {
                if DB_SOCKS5_PROXY_ENTRY.remove(&v.addr).is_err() {
                    eprintln!("db failed to delete old socks5 item: {}", &v.addr);
                }
                _deleted += 1;
            }
        })
        .await;
    eprintln!(
        "proxy check finalized: {} working, {} broken, {} deleted",
        get_all_working_proxies().len(),
        get_all_broken_proxies().len(),
        _deleted
    );

    Ok(())
}

pub async fn proxy_manager_loop() {
    loop {
        eprintln!("running proxy manager loop.");
        if proxy_manager_iteration().await.is_err() {
            eprintln!("proxy manager loop iteration failed!");
        }
        tokio::time::sleep(Duration::from_secs_f64(SCRAPER_REFRESH_SECONDS/2.0)).await;
    }
}

fn get_all_proxy_entries() -> Vec<Socks5ProxyEntry> {
    DB_SOCKS5_PROXY_ENTRY
        .iter()
        .map(|v| v.as_ref().unwrap().1.clone())
        .collect()
}

pub fn get_all_working_proxies() -> Vec<Socks5ProxyEntry> {
    get_all_proxy_entries()
        .iter()
        .filter(|&e| e.accepted).cloned()
        .collect()
}

pub fn get_all_broken_proxies() -> Vec<Socks5ProxyEntry> {
    get_all_proxy_entries()
        .iter()
        .filter(|&e| !e.accepted).cloned()
        .collect()
}

pub fn get_random_proxies(_url: &str, count: u8) -> Vec<Socks5ProxyEntry> {
    use rand::seq::SliceRandom;
    get_all_working_proxies()
        .choose_multiple(&mut rand::thread_rng(), count as usize).cloned().collect()
}

// type ValidatorFunction<T> where T: std::marker::Send + std::marker::Sync = Arc<dyn Fn(&PathBuf)->anyhow::Result<T> + std::marker::Send + std::marker::Sync + 'static>;
use tokio::task::spawn_blocking;


fn proxy_stat_increment(_type: &str, url: &str, proxy_addr: &str,proxy_cat: &str, success: bool) -> anyhow::Result<()>{
    let url_parsed = url::Url::parse(url)?;
    let url_domain = url_parsed.domain().context("url has no domain??")?;
    let stat_type = format!("proxy_{}_socksaddr_targetdomain", _type); 
    crate::config::stat_counter_increment(
        &stat_type, 
        if success {"success"} else {"fail"}, 
        proxy_addr, url_domain
    )?;
    
    let stat_type = format!("proxy_{}_sockscateg_targetdomain", _type); 
    crate::config::stat_counter_increment(
        &stat_type, 
        if success {"success"} else {"fail"}, 
        proxy_cat, url_domain
    )?;
    Ok(())
}

pub async fn download_once<T>(
    url: String,
    path: PathBuf,
    parser: (impl Fn(&PathBuf) -> anyhow::Result<T>
         + std::marker::Sync
         + std::marker::Send
         + 'static),
    socks_addr: String,
    socks_cat: String,
) -> anyhow::Result<(T, PathBuf)>
where
    T: std::marker::Send + 'static,
{   let path2 = path.clone();
    let res = crate::fetch::fetch(url.as_str(), &path, &socks_addr).await;
    proxy_stat_increment("download", url.as_str(), socks_addr.as_str(), socks_cat.as_str(), res.is_ok())?;
    res.with_context(|| format!("download error, proxy {} ({}): ", socks_addr, socks_cat))?;

    let res = spawn_blocking(move || parser(&path)).await?;
    proxy_stat_increment("parse", url.as_str(), socks_addr.as_str(), socks_cat.as_str(),res.is_ok())?;
    Ok((res.with_context(|| format!("validation error, proxy {} ({}): ", socks_addr, socks_cat))?, path2))
}

use std::path::PathBuf;
pub async fn download<T>(
    url: &str,
    path: &PathBuf,
    parser: (impl Fn(&PathBuf) -> anyhow::Result<T>
         + std::marker::Sync
         + std::marker::Send
         + 'static
         + Clone),
) -> anyhow::Result<T>
where
    T: std::marker::Send + 'static,
{
    use rand::seq::SliceRandom;
    use futures::stream::{FuturesUnordered, StreamExt};

    // if path exists, check it, if failed delete it.
    // path.exists() is sync, so do stat instead
    if tokio::fs::metadata(path).await.is_ok() {
        let parser2 = parser.clone();
        let path = path.clone();
        let path2 = path.clone();
        if let Ok(result)  = spawn_blocking(move || parser2(&path)).await? {
            return Ok(result);
        } else {
            eprintln!("DELETING existing file that failed verification: {:?}", path2.to_str());
            tokio::fs::remove_file(path2).await?;
        }
    }
    tokio::time::sleep(Duration::from_millis(1)).await;

    let tor_addr = LINKS_CONFIG
        .tor_addr_list
        .choose(&mut rand::thread_rng())
        .context("no socks proxy")?;

    let all_socks = get_random_proxies(url, LINKS_CONFIG.retries);
    let mut all_socks: Vec<(String, String)> = all_socks.iter().map(|e| (e.addr.to_owned(), e.category.to_owned())).collect();
    all_socks.push((tor_addr.clone(), "tor".to_owned()));
    let mut all_temps = vec![];
    for _ in 0..all_socks.len() {
        all_temps.push(crate::config::tempfile().await?);
    }
    let all_temps: Vec<_> = all_temps.iter().collect();

    let mut parallel_tasks = FuturesUnordered::new();

    for ((socks_addr, socks_cat), temp) in all_socks.iter().zip(all_temps) {
        let temp_path = temp.file_path().clone();
        let url = url.to_owned();
        let task = tokio::task::spawn(download_once::<T>(
            url.clone(),
            temp_path,
            parser.clone(),
            socks_addr.clone(), socks_cat.clone(),
        ));
        parallel_tasks.push(task);

        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    // extract the good result
    let mut _ok_result = None;
    let mut _errors = vec![];
    loop {
        tokio::time::sleep(Duration::from_millis(1)).await;
        match parallel_tasks.next().await {
            Some(result) => {
                let result = result.context("cannot obtain finalized result")?;
                if let Err(err) = result {
                    _errors.push(err);
                } else {
                    _ok_result = Some(result.unwrap());
                    break;
                }
            }
            None => {
                break;
            }
        }
    }
    if let Some((check_result, good_path)) = _ok_result {
        // kill all the next results
        parallel_tasks.iter().for_each(|f| f.abort());

        tokio::fs::rename(good_path, path).await?;
        return Ok(check_result);
    }

    anyhow::bail!("err: cannot download. see below: \n\n {:?}", _errors);
}
