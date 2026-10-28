use crate::{ChapterAvailability, QIDIAN_SESSION_SOURCE, ReadingChapter, Result};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

pub(crate) fn annotate(
    db: &Connection,
    book: &str,
    source: &str,
    restricted: &HashSet<String>,
    chapters: &mut [ReadingChapter],
) -> Result<()> {
    // Latest evidence is chosen per chapter: a later narrow task must not erase
    // the failure of a chapter outside its range. Rowid breaks timestamp ties.
    let mut query = db.prepare(
        "SELECT chapter_id,job_id,state,error,job_state FROM (
           SELECT i.chapter_id,i.job_id,i.state,i.error,j.state AS job_state,
             ROW_NUMBER() OVER(PARTITION BY i.chapter_id ORDER BY j.rowid DESC) AS rank
           FROM job_items i JOIN jobs j ON j.id=i.job_id WHERE j.book_id=?1
         ) WHERE rank=1",
    )?;
    let evidence = query
        .query_map([book], |r| {
            Ok((
                r.get::<_, String>(0)?,
                (
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, String>(4)?,
                ),
            ))
        })?
        .collect::<std::result::Result<HashMap<_, _>, _>>()?;
    let supported = crate::sources::download_supported(source)
        || source == QIDIAN_SESSION_SOURCE
        || source == "local-json";
    for chapter in chapters {
        let item = evidence.get(&chapter.id);
        let (state, reason) = if chapter.saved {
            ("saved", "正文已保存在本地，可离线阅读与导出".to_owned())
        } else if !supported {
            (
                "unsupported",
                "当前来源未开放正文保存，目录不代表正文可读".to_owned(),
            )
        } else if restricted.contains(&chapter.id) {
            (
                "restricted",
                item.and_then(|i| i.2.as_ref()).map_or_else(
                    || "最近确认目录标记受限；可能为付费、锁定或渠道限制，请核对官网".to_owned(),
                    |reason| format!("最近确认目录标记受限；最近任务原因：{reason}"),
                ),
            )
        } else if let Some((_, state, error, job_state)) = item {
            match state.as_str() {
                "failed" => (
                    "failed",
                    error
                        .clone()
                        .unwrap_or_else(|| "获取失败，请查看任务记录".into()),
                ),
                "blocked" => (
                    "restricted",
                    error
                        .clone()
                        .unwrap_or_else(|| "访问受限，请核对官网及当前阅读权益".into()),
                ),
                "queued" => match job_state.as_str() {
                    "queued" | "running" => {
                        ("pending", "已进入任务，等待保存；不表示正文已获取".into())
                    }
                    "paused" => ("paused", "原任务已暂停，尚未保存此章".into()),
                    "cancelled" => ("cancelled", "原任务已取消，尚未保存此章".into()),
                    _ => ("unattempted", "原任务已停止，尚未执行到此章".into()),
                },
                _ => ("unverified", "未找到本地正文，请核对任务与来源".into()),
            }
        } else if source == QIDIAN_SESSION_SOURCE {
            (
                "unattempted",
                "尚未保存；需在官网核对登录与本章阅读权益，不能仅凭目录判断".into(),
            )
        } else {
            (
                "unattempted",
                "尚未尝试，可能未在此前选章范围；目录开放标记不代表正文已验证".into(),
            )
        };
        chapter.availability = Some(ChapterAvailability {
            state: state.into(),
            reason,
            job_id: item.map(|i| i.0.clone()),
        });
    }
    Ok(())
}
