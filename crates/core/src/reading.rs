use crate::{
    BookDetails, Error, ExportRecord, ImportChapter, Library, ReadingChapter, Result,
    sources::Catalog,
};
use rusqlite::OptionalExtension;

impl Library {
    pub fn book_details(&self, book_id: &str) -> Result<BookDetails> {
        let book = self
            .snapshot()?
            .books
            .into_iter()
            .find(|book| book.id == book_id)
            .ok_or_else(|| Error::Invalid("作品不存在".into()))?;
        let connection = self.connection()?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT catalog FROM source_catalogs WHERE book_id=?1",
                [book_id],
                |r| r.get(0),
            )
            .optional()?;
        let saved = connection
            .prepare("SELECT chapter_id FROM chapters WHERE book_id=?1")?
            .query_map([book_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
        let mut restricted = std::collections::HashSet::new();
        let (description, mut chapters) = if let Some(raw) = raw {
            let catalog: Catalog = serde_json::from_str(&raw)?;
            restricted.extend(
                catalog
                    .chapters
                    .iter()
                    .filter(|c| c.restricted)
                    .map(|c| c.id.clone()),
            );
            (
                catalog.description,
                catalog
                    .chapters
                    .into_iter()
                    .enumerate()
                    .map(|(position, c)| ReadingChapter {
                        saved: saved.contains(&c.id),
                        availability: None,
                        id: c.id,
                        title: c.title,
                        position: position as u32 + 1,
                    })
                    .collect(),
            )
        } else {
            let chapters=connection.prepare("SELECT i.chapter_id,COALESCE(c.title,json_extract(i.payload,'$.title')),i.position FROM job_items i JOIN jobs j ON j.id=i.job_id LEFT JOIN chapters c ON c.book_id=j.book_id AND c.chapter_id=i.chapter_id WHERE j.book_id=?1 ORDER BY i.position")?.query_map([book_id],|r|{let id:String=r.get(0)?;Ok(ReadingChapter{saved:saved.contains(&id),availability:None,id,title:r.get(1)?,position:r.get::<_,u32>(2)?+1})})?.collect::<std::result::Result<Vec<_>,_>>()?;
            (String::new(), chapters)
        };
        crate::availability::annotate(
            &connection,
            book_id,
            &book.source,
            &restricted,
            &mut chapters,
        )?;
        let acquired_at = connection
            .query_row(
                "SELECT MAX(created_at) FROM jobs WHERE book_id=?1",
                [book_id],
                |r| r.get::<_, Option<String>>(0),
            )?
            .unwrap_or_default();
        let exports=connection.prepare("SELECT id,book_id,job_id,path,format,saved,total,partial,created_at FROM exports WHERE book_id=?1 ORDER BY rowid DESC LIMIT 50")?.query_map([book_id],|r|Ok(ExportRecord{id:r.get(0)?,book_id:r.get(1)?,job_id:r.get(2)?,path:r.get(3)?,format:r.get(4)?,saved:r.get(5)?,total:r.get(6)?,partial:r.get(7)?,created_at:r.get(8)?}))?.collect::<std::result::Result<Vec<_>,_>>()?;
        Ok(BookDetails {
            book,
            description,
            acquired_at,
            chapters,
            exports,
        })
    }

    pub fn read_chapter(&self, book_id: &str, chapter_id: &str) -> Result<ImportChapter> {
        let raw = self
            .connection()?
            .query_row(
                "SELECT title,paragraphs FROM chapters WHERE book_id=?1 AND chapter_id=?2",
                [book_id, chapter_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| Error::Invalid("该章尚未保存，无法离线阅读".into()))?;
        Ok(ImportChapter {
            id: chapter_id.into(),
            title: raw.0,
            paragraphs: serde_json::from_str(&raw.1)?,
        })
    }

    pub fn export_location(&self, id: &str) -> Result<String> {
        self.connection()?
            .query_row("SELECT path FROM exports WHERE id=?1", [id], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::Invalid("导出记录不存在".into()))
    }
}
