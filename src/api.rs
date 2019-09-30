use crate::error::{SimpleError, SimpleResult};
use chrono::prelude::*;
use futures::future::Future;
use reqwest::header;
use reqwest::header::HeaderMap;
use reqwest::r#async::Client;
use serde::{Deserialize, Serialize};
use serde_repr::*;
use std::time::Duration;

#[derive(Debug)]
pub struct FojApi {
    token: String,
    client: Client,
}

impl FojApi {
    pub fn new(token: String) -> SimpleResult<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, format!("token={}", token).parse().unwrap());

        let client = Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        Ok(FojApi { token, client })
    }

    pub fn session(&self) -> impl Future<Item = Session, Error = SimpleError> {
        self.client
            .get("https://api.oj.nctu.me/session/")
            .send()
            .and_then(|res| res.error_for_status())
            .and_then(|mut res| res.json())
            .map_err(|e| e.into())
            .and_then(|msg: Msg<Session>| Ok(msg.unwrap()))
    }

    pub fn get_problem_list(
        &self,
        group_id: u32,
    ) -> impl Future<Item = Vec<Problem>, Error = SimpleError> {
        self.client
            .get(format!("https://api.oj.nctu.me/groups/{}/problems/", group_id).as_str())
            .query(&[("group_id", group_id.to_string())])
            .query(&[("count", 10000.to_string())])
            .query(&[("page", 1.to_string())])
            .send()
            .and_then(|res| res.error_for_status())
            .and_then(|mut res| res.json())
            .map_err(|e| e.into())
            .map(|msg: Msg<ProblemList>| msg.unwrap().data)
    }

    pub fn get_submission_group(
        &self,
        group_id: u32,
    ) -> impl Future<Item = Vec<Submission>, Error = SimpleError> {
        self.get_submission(group_id, 1_000_000, 1, None, None, None)
            .map(|res: (usize, Vec<Submission>)| res.1)
    }

    pub fn get_submission_prob(
        &self,
        group_id: u32,
        pid: u32,
    ) -> impl Future<Item = Vec<Submission>, Error = SimpleError> {
        self.get_submission(group_id, 1_000_000, 1, Some(pid), None, None)
            .map(|res: (usize, Vec<Submission>)| res.1)
    }

    fn get_submission(
        &self,
        group_id: u32,
        count: usize,
        page: u32,
        pid: Option<u32>,
        name: Option<&str>,
        verdict: Option<Verdict>,
    ) -> impl Future<Item = (usize, Vec<Submission>), Error = SimpleError> {
        let mut builder = self
            .client
            .get("https://api.oj.nctu.me/submissions/")
            .query(&[("group_id", group_id.to_string())])
            .query(&[("count", count.to_string())])
            .query(&[("page", page.to_string())]);
        if let Some(pid) = pid {
            builder = builder.query(&[("problem_id", pid.to_string())])
        }
        if let Some(name) = name {
            builder = builder.query(&[("name", name)])
        }
        if let Some(verdict) = verdict {
            builder = builder.query(&[("verdict_id", (verdict as u32).to_string())])
        }
        builder
            .send()
            .and_then(|res| res.error_for_status())
            .and_then(|mut res| res.json())
            .map_err(|e| e.into())
            .and_then(|msg: Msg<SubmissionList>| Ok((msg.msg.count as usize, msg.msg.submissions)))
    }

    pub fn get_user_name(&self, user_id: u32) -> impl Future<Item = String, Error = SimpleError> {
        self.client
            .get(format!("https://api.oj.nctu.me/users/{}/", user_id).as_str())
            .send()
            .and_then(|res| res.error_for_status())
            .and_then(|mut res| res.json())
            .map_err(|e| e.into())
            .and_then(|msg: Msg<UserName>| Ok(msg.unwrap().name))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub email: String,
    pub id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SubmissionList {
    count: usize,
    submissions: Vec<Submission>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProblemList {
    count: usize,
    data: Vec<Problem>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize_repr, Serialize_repr)]
#[repr(u32)]
pub enum Verdict {
    Pending = 1,
    Judging = 2,
    SE = 3,
    CE = 4,
    RE = 5,
    MLE = 6,
    TLE = 7,
    OLE = 8,
    WA = 9,
    AC = 10,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Submission {
    pub memory_usage: Option<u64>,
    pub time_usage: Option<u64>,
    pub length: usize,
    pub verdict_id: Verdict,
    pub execute_id: u32,
    pub user_id: u32,
    pub problem_id: u32,
    #[serde(with = "simple_datetime")]
    pub created_at: DateTime<Local>,
    #[serde(with = "simple_datetime")]
    pub updated_at: DateTime<Local>,
    pub id: u64,
    pub score: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Problem {
    pub id: u32,
    pub status: i32,
    pub title: String,
    pub source: String,
    pub user_id: u32,
    pub visible: bool,
    pub group_read: bool,
    pub group_write: bool,
}

// This module is modified from serde's example
// See https://serde.rs/custom-date-format.html
mod simple_datetime {
    use chrono::{DateTime, Local, TimeZone};
    use serde::{self, Deserialize, Deserializer, Serializer};

    const FORMAT: &str = "%Y-%m-%d %H:%M:%S";

    pub fn serialize<S>(date: &DateTime<Local>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", date.format(FORMAT));
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Local>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Local
            .datetime_from_str(&s, FORMAT)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct UserName {
    name: String,
    id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
struct Msg<M> {
    msg: M,
}

impl<M> Msg<M> {
    pub fn unwrap(self) -> M {
        self.msg
    }
}
