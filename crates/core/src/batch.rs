use crate::{
    Error, Library, Result,
    sources::{SourceProvider, download_supported, identify},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
pub const MAX_BATCH_ITEMS: usize = 10;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub position: usize,
    pub input: String,
    pub state: String,
    pub job_id: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSnapshot {
    pub id: String,
    pub state: String,
    pub selection: String,
    pub duplicates: usize,
    pub error: Option<String>,
    pub items: Vec<BatchItem>,
}
impl Library {
    pub fn create_batch(&self, inputs: &[String], selection: &str) -> Result<String> {
        if selection.len() > 2048 {
            return Err(Error::Invalid("选章表达式过长".into()));
        }
        if inputs.is_empty() || inputs.len() > MAX_BATCH_ITEMS {
            return Err(Error::Invalid("每批须为 1～10 部作品".into()));
        }
        // Validate selection syntax before any persistent mutation. Bounds are checked against
        // each actual catalog; a too-short book becomes an explicit per-item result.
        crate::parse_selection(selection, crate::MAX_CHAPTERS)?;
        let mut unique = std::collections::HashSet::new();
        let mut normalized = Vec::new();
        for input in inputs {
            if input.len() > 2048 {
                return Err(Error::Invalid("作品链接过长".into()));
            }
            let (source, id) = identify(input)?;
            if !download_supported(source) {
                return Err(Error::Invalid("批次包含尚未开放正文下载的来源".into()));
            }
            let canonical = format!("{}:{id}", source.trim_end_matches("-web"));
            if unique.insert(canonical.clone()) {
                normalized.push(canonical);
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let active: u32 = tx.query_row(
            "SELECT COUNT(*) FROM batches WHERE state IN ('queued','running','paused')",
            [],
            |r| r.get(0),
        )?;
        if active >= 5 {
            return Err(Error::Conflict(
                "最多保留 5 个待处理批次，请先完成或取消".into(),
            ));
        }
        tx.execute(
            "INSERT INTO batches(id,state,selection,duplicates) VALUES(?1,'queued',?2,?3)",
            params![id, selection, inputs.len() - normalized.len()],
        )?;
        for (position, input) in normalized.iter().enumerate() {
            tx.execute(
                "INSERT INTO batch_items(batch_id,position,input,state) VALUES(?1,?2,?3,'queued')",
                params![id, position, input],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }
    pub fn batches(&self) -> Result<Vec<BatchSnapshot>> {
        let ids = self
            .connection()?
            .prepare("SELECT id FROM batches ORDER BY rowid DESC")?
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| self.batch(id)).collect()
    }
    pub fn batch(&self, id: &str) -> Result<BatchSnapshot> {
        let db = self.connection()?;
        let header = db
            .query_row(
                "SELECT state,selection,duplicates,error FROM batches WHERE id=?1",
                [id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, usize>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("批次不存在".into()))?;
        let items=db.prepare("SELECT position,input,state,job_id,error FROM batch_items WHERE batch_id=?1 ORDER BY position")?.query_map([id],|r|Ok(BatchItem{position:r.get::<_,usize>(0)?+1,input:r.get(1)?,state:r.get(2)?,job_id:r.get(3)?,error:r.get(4)?}))?.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok(BatchSnapshot {
            id: id.into(),
            state: header.0,
            selection: header.1,
            duplicates: header.2,
            error: header.3,
            items,
        })
    }
    pub fn stop_batch(&self, id: &str, paused: bool, token: Option<&AtomicBool>) -> Result<()> {
        let mut db = self.connection()?;
        let tx = db.transaction()?;
        let state = if paused { "paused" } else { "cancelled" };
        if tx.execute("UPDATE batches SET state=?2 WHERE id=?1 AND (state IN ('queued','running') OR (?2='cancelled' AND state='paused'))",params![id,state])?!=1 {return Err(Error::Conflict("批次不在可停止状态".into()));}
        tx.execute("UPDATE jobs SET state=?2,error=NULL WHERE id IN (SELECT job_id FROM batch_items WHERE batch_id=?1 AND state='running') AND state IN ('queued','running','paused')",params![id,state])?;
        if let Some(token) = token {
            token.store(true, Ordering::Release);
        }
        tx.commit()?;
        Ok(())
    }
    pub fn active_batch_for_job(&self, job_id: &str) -> Result<Option<String>> {
        Ok(self.connection()?.query_row("SELECT b.id FROM batches b JOIN batch_items i ON i.batch_id=b.id WHERE i.job_id=?1 AND i.state='running' AND b.state IN ('queued','running','paused')",[job_id],|r|r.get(0)).optional()?)
    }
    pub fn record_batch_failure(&self, id: &str, error: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE batches SET state='paused',error=?2 WHERE id=?1 AND state='running'",
            params![id, error],
        )?;
        Ok(())
    }
    fn batch_current(&self, id: &str, generation: u64) -> Result<bool> {
        Ok(self.connection()?.query_row(
            "SELECT state='running' AND generation=?2 FROM batches WHERE id=?1",
            params![id, generation],
            |r| r.get(0),
        )?)
    }
    /// Sequential books share one provider/rate limiter. Each job link is created in the same
    /// transaction as the job, so restart cannot create a second orphan download for that item.
    pub fn run_batch(
        &self,
        id: &str,
        provider: &dyn SourceProvider,
        stop: &AtomicBool,
    ) -> Result<BatchSnapshot> {
        let generation = {
            let mut db = self.connection()?;
            let tx = db.transaction()?;
            if tx.execute("UPDATE batches SET state='running',error=NULL,generation=generation+1 WHERE id=?1 AND state IN ('queued','paused') AND NOT EXISTS(SELECT 1 FROM batches other WHERE other.id!=batches.id AND other.state='running')",[id])?!=1 {return Err(Error::Conflict("已有批次正在执行，或者本批次已结束/不存在".into()));}
            let n = tx.query_row("SELECT generation FROM batches WHERE id=?1", [id], |r| {
                r.get::<_, u64>(0)
            })?;
            tx.commit()?;
            n
        };
        let snapshot = self.batch(id)?;
        for item in snapshot
            .items
            .iter()
            .filter(|i| matches!(i.state.as_str(), "queued" | "running"))
        {
            if stop.load(Ordering::Acquire) || !self.batch_current(id, generation)? {
                break;
            }
            let result = (|| -> Result<crate::JobSnapshot> {
                let job = if let Some(job) = &item.job_id {
                    job.clone()
                } else {
                    let catalog = provider.inspect(&item.input)?;
                    if stop.load(Ordering::Acquire) || !self.batch_current(id, generation)? {
                        return Err(Error::Cancelled);
                    }
                    let positions =
                        crate::parse_selection(&snapshot.selection, catalog.chapters.len())?;
                    self.create_download_linked(
                        &catalog,
                        &positions,
                        Some((id, item.position - 1, generation)),
                    )?
                };
                let current = self.job(&job)?;
                if current.state == "completed" {
                    Ok(current)
                } else {
                    self.run_batch_job(&job, provider.adapter(&current.source)?, stop)
                }
            })();
            if stop.load(Ordering::Acquire) || !self.batch_current(id, generation)? {
                break;
            }
            let (state, error, pause) = match result {
                Ok(job) => {
                    let network = self
                        .job_details(&job.id)?
                        .items
                        .iter()
                        .any(|i| i.error_kind.as_deref() == Some("network"));
                    let pause = network || job.state == "paused";
                    (
                        if pause {
                            "running".to_owned()
                        } else {
                            job.state
                        },
                        job.error,
                        pause,
                    )
                }
                Err(Error::Network(message)) => (
                    item.state.clone(),
                    Some(format!("网络请求失败：{message}")),
                    true,
                ),
                Err(Error::Cancelled) => (item.state.clone(), None, true),
                Err(error) => ("failed".into(), Some(error.to_string()), false),
            };
            self.connection()?.execute("UPDATE batch_items SET state=?3,error=?4 WHERE batch_id=?1 AND position=?2 AND EXISTS(SELECT 1 FROM batches WHERE id=?1 AND state='running' AND generation=?5)",params![id,item.position-1,state,error,generation])?;
            if pause {
                self.connection()?.execute("UPDATE batches SET state='paused' WHERE id=?1 AND state='running' AND generation=?2",params![id,generation])?;
                break;
            }
        }
        let db = self.connection()?;
        db.execute("UPDATE batches SET state=CASE WHEN EXISTS(SELECT 1 FROM batch_items WHERE batch_id=?1 AND state IN ('queued','running')) THEN 'paused' WHEN EXISTS(SELECT 1 FROM batch_items WHERE batch_id=?1 AND state!='completed') THEN 'partial' ELSE 'completed' END WHERE id=?1 AND state='running' AND generation=?2",params![id,generation])?;
        drop(db);
        self.batch(id)
    }
}
