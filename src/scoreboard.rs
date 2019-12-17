use crate::api::*;
use anyhow::Result as AnyResult;
use async_std::fs;
use async_std::path::Path;
use async_std::prelude::*;
use chrono::prelude::*;
use futures::stream::{FuturesUnordered, StreamExt as _};
use prettytable::{format::Alignment, Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Serialize, Deserialize)]
pub struct Scoreboard {
    user_map: Mutex<BTreeMap<u32, UserRecord>>,
    problem_set: Mutex<BTreeSet<u32>>,
    cache_time: RwLock<DateTime<Local>>,
}

impl Scoreboard {
    pub fn new() -> Self {
        Self {
            user_map: Mutex::new(BTreeMap::new()),
            problem_set: Mutex::new(BTreeSet::new()),
            cache_time: RwLock::new(DateTime::<Local>::from(std::time::UNIX_EPOCH)),
        }
    }

    pub async fn load_cache<P: AsRef<Path>>(path: P) -> AnyResult<Self> {
        let mut f = fs::OpenOptions::new().read(true).open(path).await?;
        trace!("Deserializing file: {:?}", &f);
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await?;
        Ok(bincode::deserialize(&buf)?)
    }

    pub async fn save_cache<P: AsRef<Path>>(&self, path: P) -> AnyResult<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .await?;
        trace!("Serializing file: {:?}", &f);
        let buf = bincode::serialize(self)?;
        f.write_all(&buf).await?;
        Ok(())
    }

    pub fn probset(&self) -> BTreeSet<u32> {
        self.problem_set.lock().unwrap().clone()
    }

    pub fn gen_table(&self, problems: &BTreeSet<u32>) -> Table {
        debug!("Generating table...");
        let mut table = Table::new();
        let user_lock = self.user_map.lock().unwrap();
        let mut users: Vec<&UserRecord> = user_lock.iter().map(|p| p.1).collect();

        // Generate the displaying problem list
        debug!("Displaying problem list: {:?}", problems);

        users.sort_by(|&a, &b| b.ac_count(problems).cmp(&a.ac_count(problems)));

        // Generate problems' ID
        let mut prob_cells = Vec::new();
        prob_cells.push(cell!(""));
        for prob in problems.iter() {
            prob_cells.push(cell!(c->prob));
        }
        table.add_row(Row::new(prob_cells.clone()));

        // Generate Update Time
        let mut update_row = Vec::new();
        update_row.push(cell!(c->"Updated At"));

        let t = self.cache_time.read().unwrap();
        let mut update_cell = Cell::new_align(
            format!("{}\n{}", t.format("%Y-%m-%d"), t.format("%H:%M:%S")).as_str(),
            Alignment::CENTER,
        );
        update_cell.set_hspan(problems.len());
        update_row.push(update_cell);

        table.add_row(Row::new(update_row));

        // Generate User Solving Status
        for user in &users {
            let mut cells = Vec::new();
            // Hide all 'NS' user
            let mut should_display = false;
            cells.push(cell!(c->user.name));
            for prob in problems.iter() {
                let p = &user.problems.get(&prob).copied().unwrap_or_default();
                let c = match p.status {
                    SolveStatus::Accepted => {
                        should_display = true;
                        cell!(Fgc->format!("{} / {}", p.status, p.wa_count + 1))
                    }
                    SolveStatus::WrongAnswer => {
                        should_display = true;
                        cell!(Frc->format!("{} / {}", p.status, p.wa_count))
                    }
                    SolveStatus::None => cell!(FDc->format!("{}", p.status)),
                };
                cells.push(c);
            }
            if should_display {
                table.add_row(Row::new(cells));
            }
        }

        // Also generate a column at the footer
        table.add_row(Row::new(prob_cells));

        table
    }

    pub async fn fetch(self: Arc<Self>, gid: u32, token: String) -> AnyResult<()> {
        debug!("Starting to fetch submission...");
        let foj = FojApi::new(token)?;

        // Authentication
        let session = foj
            .session()
            .await
            .map_err::<anyhow::Error, _>(|_| anyhow!("Authentication Failed!"))?;
        debug!("Authentication Succuss!");
        trace!("User session: {:?}", session);

        // Fetch
        fetch_group(self.clone(), foj.clone(), gid).await?;
        fetch_names(self, foj).await?;
        Ok(())
    }
}

async fn fetch_group(board: Arc<Scoreboard>, foj: FojApi, gid: u32) -> AnyResult<()> {
    trace!("Fetching submissions in group {}...", gid);
    let mut submissions = foj.get_submission_group(gid).await?;
    trace!("Fetched {} submissions.", submissions.len());
    submissions.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    save_submissions(board, submissions)?;
    Ok(())
}

fn save_submissions(board: Arc<Scoreboard>, submissions: Vec<Submission>) -> AnyResult<()> {
    let time_lock = board.cache_time.read().unwrap();
    let mut new_time = *time_lock;

    trace!("Filter submissions created before: {}", time_lock);
    let start_from = match submissions.binary_search_by(|sub| sub.created_at.cmp(&time_lock)) {
        Ok(p) => p + 1,
        Err(p) => p,
    };
    trace!(
        "Starting from submission: {:?}",
        submissions.get(start_from).map(|sub| sub.id)
    );
    trace!(
        "Submissions to be processed: {}",
        &submissions[start_from..].len()
    );

    let mut user_lock = board.user_map.lock().unwrap();
    let mut problems_lock = board.problem_set.lock().unwrap();

    for sub in &submissions[start_from..] {
        let user_record: &mut UserRecord = user_lock.entry(sub.user_id).or_default();
        let pid = sub.problem_id;

        if !problems_lock.contains(&pid) {
            problems_lock.insert(pid);
        }

        match sub.verdict_id as u32 {
            4..=9 => {
                if user_record.problem(pid).status != SolveStatus::Accepted {
                    user_record.problem(pid).status = SolveStatus::WrongAnswer;
                    user_record.problem(pid).wa_count += 1;
                }
                if sub.created_at > new_time {
                    new_time = sub.created_at;
                }
            }
            10 => {
                user_record.problem(pid).status = SolveStatus::Accepted;
                if sub.created_at > new_time {
                    new_time = sub.created_at;
                }
            }
            _ => {}
        }
    }

    drop(time_lock);
    let mut time_entry = board.cache_time.write().unwrap();
    if new_time > *time_entry {
        *time_entry = new_time;
    }
    Ok(())
}

async fn fetch_names(board: Arc<Scoreboard>, foj: FojApi) -> AnyResult<()> {
    let futures_iter: FuturesUnordered<_> = board
        .user_map
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(&uid, user)| {
            if user.name.is_empty() {
                Some(update_name(board.clone(), foj.clone(), uid))
            } else {
                None
            }
        })
        .collect();
    if log_enabled!(log::Level::Trace) {
        trace!(
            "There are {} users' name going to be updated.",
            futures_iter.len()
        );
    }

    let results: Vec<AnyResult<()>> = futures_iter.collect().await;
    results.into_iter().collect::<AnyResult<()>>()?;
    Ok(())
}

async fn update_name(board: Arc<Scoreboard>, foj: FojApi, uid: u32) -> AnyResult<()> {
    trace!("Fetching the name of user {}...", uid);
    let name = foj.get_user_name(uid).await?;
    trace!("user {} => {}", uid, &name);
    board
        .user_map
        .lock()
        .unwrap()
        .entry(uid)
        .and_modify(|user| {
            user.name = name;
        });
    Ok(())
}

impl Default for Scoreboard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UserRecord {
    id: u32,
    name: String,
    problems: BTreeMap<u32, ProblemCell>,
}

impl UserRecord {
    fn ac_count(&self, prob_set: &BTreeSet<u32>) -> usize {
        let mut count = 0;
        for prob in prob_set {
            if let Some(cell) = self.problems.get(prob) {
                if cell.status == SolveStatus::Accepted {
                    count += 1;
                }
            }
        }
        count
    }

    fn problem(&mut self, prob_id: u32) -> &mut ProblemCell {
        self.problems.entry(prob_id).or_default()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct ProblemCell {
    wa_count: usize,
    status: SolveStatus,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum SolveStatus {
    None = 0,
    Accepted,
    WrongAnswer,
}

impl fmt::Display for SolveStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !f.alternate() {
            match self {
                SolveStatus::Accepted => write!(f, "AC"),
                SolveStatus::WrongAnswer => write!(f, "WA"),
                SolveStatus::None => write!(f, "NS"),
            }
        } else {
            match self {
                SolveStatus::Accepted => write!(f, "Accepted"),
                SolveStatus::WrongAnswer => write!(f, "Wrong Answer"),
                SolveStatus::None => write!(f, "None"),
            }
        }
    }
}

impl Default for SolveStatus {
    fn default() -> Self {
        SolveStatus::None
    }
}
