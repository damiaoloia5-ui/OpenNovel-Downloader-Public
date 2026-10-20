use crate::{BookContent, Error, ExportReceipt, Library, Result};
use std::{io::Write, path::Path};
use tempfile::NamedTempFile;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Txt,
    Epub,
}

impl ExportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "txt" => Ok(Self::Txt),
            "epub" => Ok(Self::Epub),
            _ => Err(Error::Invalid("导出格式必须为 txt 或 epub".into())),
        }
    }
}

impl Library {
    pub fn export(
        &self,
        book_id: &str,
        directory: impl AsRef<Path>,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        let content = self.content(book_id)?;
        self.persist_export(book_id, None, content, directory.as_ref(), format)
    }

    pub fn export_job(
        &self,
        job_id: &str,
        directory: impl AsRef<Path>,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        let job = self.job(job_id)?;
        let mut content = self.content(&job.book_id)?;
        let selected = self
            .connection()?
            .prepare("SELECT chapter_id FROM job_items WHERE job_id=?1")?
            .query_map([job_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<std::collections::HashSet<_>, _>>()?;
        content.chapters.retain(|c| selected.contains(&c.id));
        if content.chapters.is_empty() {
            return Err(Error::Conflict("所选任务尚无已保存正文".into()));
        }
        content.summary.id = job_id.into();
        content.summary.title.push_str("（任务选章）");
        content.summary.total_chapters = selected.len() as u32;
        content.summary.saved_chapters = content.chapters.len() as u32;
        self.persist_export(
            &job.book_id,
            Some(job_id),
            content,
            directory.as_ref(),
            format,
        )
    }

    fn persist_export(
        &self,
        book_id: &str,
        job_id: Option<&str>,
        content: BookContent,
        directory: &Path,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        std::fs::create_dir_all(directory)?;
        let partial = content.summary.saved_chapters != content.summary.total_chapters;
        let extension = match format {
            ExportFormat::Txt => "txt",
            ExportFormat::Epub => "epub",
        };
        let suffix = if partial {
            format!(
                "-partial-{}-of-{}",
                content.summary.saved_chapters, content.summary.total_chapters
            )
        } else if content.summary.source == "fanqie-web" {
            format!("-complete-{}", content.summary.total_chapters)
        } else {
            String::new()
        };
        let prefix = if job_id.is_some() { "job-" } else { "" };
        let path = std::path::absolute(directory.join(format!(
            "{prefix}{}{suffix}.{extension}",
            content.summary.id
        )))?;
        let mut temporary = NamedTempFile::new_in(directory)?;
        match format {
            ExportFormat::Txt => write_txt(&mut temporary, &content)?,
            ExportFormat::Epub => write_epub(temporary.as_file_mut(), &content)?,
        }
        temporary.as_file_mut().flush()?;
        temporary.as_file().sync_all()?;
        temporary
            .persist_noclobber(&path)
            .map_err(|error| Error::Io(error.error))?;
        self.connection()?.execute("INSERT INTO exports (id,book_id,job_id,path,format,saved,total,partial) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",rusqlite::params![uuid::Uuid::new_v4().to_string(),book_id,job_id,path.to_string_lossy(),extension,content.summary.saved_chapters,content.summary.total_chapters,partial])?;
        Ok(ExportReceipt {
            path: path.to_string_lossy().into_owned(),
            saved_chapters: content.summary.saved_chapters,
            total_chapters: content.summary.total_chapters,
            partial,
        })
    }
}

fn coverage(content: &BookContent) -> String {
    format!(
        "本地保存 {} / {} 章{}",
        content.summary.saved_chapters,
        content.summary.total_chapters,
        if content.summary.saved_chapters < content.summary.total_chapters {
            "（部分内容）"
        } else {
            ""
        }
    )
}

fn write_txt(output: &mut impl Write, content: &BookContent) -> Result<()> {
    writeln!(
        output,
        "{}\n作者：{}\n来源：{}\n{}\n{}\n",
        content.summary.title,
        content.summary.author,
        content.summary.source_label(),
        content.summary.source_url().unwrap_or_default(),
        coverage(content)
    )?;
    for chapter in &content.chapters {
        writeln!(output, "{}\n", chapter.title)?;
        for paragraph in &chapter.paragraphs {
            writeln!(output, "{paragraph}\n")?;
        }
    }
    Ok(())
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn write_epub(output: &mut std::fs::File, content: &BookContent) -> Result<()> {
    let mut archive = ZipWriter::new(output);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let compressed = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("mimetype", stored)?;
    archive.write_all(b"application/epub+zip")?;
    archive.start_file("META-INF/container.xml", compressed)?;
    archive.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)?;
    let title = xml(&content.summary.title);
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>",
    );
    let mut spine = String::new();
    let mut navigation = String::new();
    for (index, chapter) in content.chapters.iter().enumerate() {
        let name = format!("chapter-{index}.xhtml");
        manifest.push_str(&format!(
            "<item id=\"c{index}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>"
        ));
        spine.push_str(&format!("<itemref idref=\"c{index}\"/>"));
        navigation.push_str(&format!(
            "<li><a href=\"{name}\">{}</a></li>",
            xml(&chapter.title)
        ));
        let body = chapter
            .paragraphs
            .iter()
            .map(|paragraph| format!("<p>{}</p>", xml(paragraph)))
            .collect::<String>();
        archive.start_file(format!("EPUB/{name}"), compressed)?;
        write!(
            archive,
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" lang=\"zh-CN\" xml:lang=\"zh-CN\"><head><title>{}</title></head><body><h1>{}</h1>{body}</body></html>",
            xml(&chapter.title),
            xml(&chapter.title)
        )?;
    }
    archive.start_file("EPUB/nav.xhtml", compressed)?;
    write!(
        archive,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" lang=\"zh-CN\" xml:lang=\"zh-CN\"><head><title>目录</title></head><body><h1>{title}</h1><p>{}</p><nav epub:type=\"toc\" id=\"toc\"><h2>目录</h2><ol>{navigation}</ol></nav></body></html>",
        xml(&coverage(content))
    )?;
    archive.start_file("EPUB/package.opf", compressed)?;
    let modified = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let creator = if content.summary.author.is_empty() {
        String::new()
    } else {
        format!("<dc:creator>{}</dc:creator>", xml(&content.summary.author))
    };
    let source = content
        .summary
        .source_url()
        .map(|url| format!("<dc:source>{}</dc:source>", xml(&url)))
        .unwrap_or_default();
    write!(
        archive,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"book-id\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"book-id\">urn:uuid:{}</dc:identifier><dc:title>{title}</dc:title>{creator}{source}<dc:language>zh-CN</dc:language><dc:description>{}</dc:description><meta property=\"dcterms:modified\">{modified}</meta></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>",
        content.summary.id,
        xml(&coverage(content))
    )?;
    archive.finish()?;
    Ok(())
}
