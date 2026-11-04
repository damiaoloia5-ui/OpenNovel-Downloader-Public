use crate::{BookContent, Error, ExportReceipt, Library, Result};
use std::{io::Write, path::Path};
use tempfile::NamedTempFile;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Txt,
    Epub,
    Docx,
}

impl ExportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "txt" => Ok(Self::Txt),
            "epub" => Ok(Self::Epub),
            "docx" => Ok(Self::Docx),
            _ => Err(Error::Invalid("导出格式必须为 txt、epub 或 docx".into())),
        }
    }
}

impl Library {
    /// Generate the persisted format and scope, independently of the UI lifetime.
    pub fn export_automatic(
        &self,
        book_id: &str,
        directory: impl AsRef<Path>,
    ) -> Result<ExportReceipt> {
        let plan = self.automatic_export_plan(book_id)?;
        let format = ExportFormat::parse(&plan.format)?;
        let content = if let Some(job) = &plan.job_id {
            self.job_content(job)?
        } else {
            self.content(book_id)?
        };
        let directory = directory.as_ref();
        std::fs::create_dir_all(directory)?;
        let prepared = match format {
            ExportFormat::Txt => (txt_digest(&content)?, None),
            _ => {
                let file = render_file(directory, &content, format)?;
                (file_digest(file.path())?, Some(file))
            }
        };
        self.persist_export_inner(
            book_id,
            plan.job_id.as_deref(),
            content,
            directory,
            format,
            Some(prepared),
        )
    }
    /// Generate a TXT snapshot without overwriting any previous or edited file.
    /// Repeated calls repair a missing file/record and reuse identical bytes.
    pub fn export_automatic_txt(
        &self,
        book_id: &str,
        directory: impl AsRef<Path>,
    ) -> Result<ExportReceipt> {
        let content = self.content(book_id)?;
        let digest = txt_digest(&content)?;
        self.persist_export_inner(
            book_id,
            None,
            content,
            directory.as_ref(),
            ExportFormat::Txt,
            Some((digest, None)),
        )
    }
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
        let content = self.job_content(job_id)?;
        let book_id = self.job(job_id)?.book_id;
        self.persist_export(&book_id, Some(job_id), content, directory.as_ref(), format)
    }

    pub fn export_selected(
        &self,
        book_id: &str,
        selection: &str,
        directory: impl AsRef<Path>,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        let details = self.book_details(book_id)?;
        let positions = crate::parse_selection(selection, details.book.total_chapters as usize)?;
        let positions: std::collections::HashSet<_> = positions.into_iter().collect();
        let ids: Vec<_> = details
            .chapters
            .iter()
            .filter(|c| positions.contains(&(c.position as usize)))
            .map(|c| c.id.as_str())
            .collect();
        let mut content = self.content_selected(book_id, Some(&ids))?;
        if content.chapters.is_empty() {
            return Err(Error::Conflict("所选范围尚无已保存正文".into()));
        }
        content
            .summary
            .title
            .push_str(&format!("（目录序号 {selection}）"));
        content.summary.saved_chapters = content.chapters.len() as u32;
        content.summary.total_chapters = positions.len() as u32;
        self.persist_export(book_id, None, content, directory.as_ref(), format)
    }

    fn job_content(&self, job_id: &str) -> Result<BookContent> {
        let job = self.job(job_id)?;
        let mut content = if let Some(id) = self.job_history_id(job_id)? {
            self.historical_content(&id)?
        } else {
            self.content(&job.book_id)?
        };
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
        content.modified = self.connection()?.query_row(
            "SELECT created_at FROM jobs WHERE id=?1",
            [job_id],
            |r| r.get(0),
        )?;
        Ok(content)
    }

    pub(crate) fn persist_export(
        &self,
        book_id: &str,
        job_id: Option<&str>,
        content: BookContent,
        directory: &Path,
        format: ExportFormat,
    ) -> Result<ExportReceipt> {
        self.persist_export_inner(book_id, job_id, content, directory, format, None)
    }

    fn persist_export_inner(
        &self,
        book_id: &str,
        job_id: Option<&str>,
        content: BookContent,
        directory: &Path,
        format: ExportFormat,
        prepared: Option<(String, Option<NamedTempFile>)>,
    ) -> Result<ExportReceipt> {
        let _guard = self
            .export_lock
            .lock()
            .map_err(|_| Error::Conflict("导出锁失效，请重启程序".into()))?;
        std::fs::create_dir_all(directory)?;
        let directory = std::path::absolute(directory)?;
        let directory_identity = std::fs::canonicalize(&directory)?;
        let partial = content.summary.saved_chapters != content.summary.total_chapters;
        let extension = match format {
            ExportFormat::Txt => "txt",
            ExportFormat::Epub => "epub",
            ExportFormat::Docx => "docx",
        };
        let automatic = prepared.is_some();
        let (digest, temporary) = if let Some(prepared) = prepared {
            prepared
        } else {
            let file = render_file(&directory, &content, format)?;
            let digest = file_digest(file.path())?;
            (digest, Some(file))
        };
        let known = self.connection()?.prepare("SELECT path FROM exports WHERE book_id=?1 AND format=?2 AND automatic=?3 AND content_hash=?4 AND job_id IS ?5 ORDER BY rowid DESC")?
            .query_map(rusqlite::params![book_id,extension,automatic,digest,job_id], |r| r.get::<_,String>(0))?
            .collect::<std::result::Result<Vec<_>,_>>()?
            .into_iter().map(std::path::PathBuf::from)
            .find(|p| p.parent().and_then(|d| std::fs::canonicalize(d).ok()).as_ref() == Some(&directory_identity));
        let path = if let Some(path) = known {
            if path.try_exists()? {
                if !automatic {
                    return Err(export_exists(&path));
                }
                if file_digest(&path)? != digest {
                    return Err(Error::Conflict(
                        "自动导出的目标文件已被修改，已保留；请先移走该文件或选择其他文件夹再重试"
                            .into(),
                    ));
                }
            }
            path
        } else {
            let stem = crate::export_settings::book_filename(&self.book_summary(book_id)?);
            let mut selected = None;
            for number in 1..=10000 {
                let suffix = if number == 1 {
                    String::new()
                } else {
                    format!("（{number}）")
                };
                let path = directory.join(format!("{stem}{suffix}.{extension}"));
                if !path.try_exists()? {
                    selected = Some(path);
                    break;
                }
                let recorded: bool = self.connection()?.query_row(
                    "SELECT EXISTS(SELECT 1 FROM exports WHERE path=?1)",
                    [path.to_string_lossy()],
                    |r| r.get(0),
                )?;
                // Reconcile an orphan created just before a database write failure.
                if automatic && !recorded && path.is_file() && file_digest(&path)? == digest {
                    selected = Some(path);
                    break;
                }
            }
            selected.ok_or_else(|| Error::Conflict("同名版本过多，请选择其他文件夹".into()))?
        };
        if !path.try_exists()? {
            let file = match temporary {
                Some(file) => file,
                None => render_file(&directory, &content, format)?,
            };
            if file_digest(file.path())? != digest {
                return Err(Error::Conflict("导出内容生成校验失败".into()));
            }
            file.persist_noclobber(&path).map_err(|error| {
                if path.exists() {
                    export_exists(&path)
                } else {
                    Error::Io(error.error)
                }
            })?;
        }
        self.connection()?.execute("INSERT INTO exports (id,book_id,job_id,path,format,saved,total,partial,content_hash,automatic) SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10 WHERE NOT EXISTS (SELECT 1 FROM exports WHERE book_id=?2 AND path=?4)",rusqlite::params![uuid::Uuid::new_v4().to_string(),book_id,job_id,path.to_string_lossy(),extension,content.summary.saved_chapters,content.summary.total_chapters,partial,digest,automatic])?;
        Ok(ExportReceipt {
            path: path.to_string_lossy().into_owned(),
            saved_chapters: content.summary.saved_chapters,
            total_chapters: content.summary.total_chapters,
            partial,
        })
    }
}

#[derive(Default)]
struct HashWriter(sha2::Sha256);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl HashWriter {
    fn finish(self) -> String {
        use sha2::Digest;
        format!("{:x}", self.0.finalize())
    }
}
fn txt_digest(content: &BookContent) -> Result<String> {
    let mut hash = HashWriter::default();
    write_txt(&mut hash, content)?;
    Ok(hash.finish())
}

fn file_digest(path: &Path) -> Result<String> {
    let mut hash = HashWriter::default();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hash)?;
    Ok(hash.finish())
}

fn render_file(
    directory: &Path,
    content: &BookContent,
    format: ExportFormat,
) -> Result<NamedTempFile> {
    let mut file = NamedTempFile::new_in(directory)?;
    match format {
        ExportFormat::Txt => write_txt(&mut file, content)?,
        ExportFormat::Epub => write_epub(file.as_file_mut(), content)?,
        ExportFormat::Docx => crate::docx::write_docx(file.as_file_mut(), content)?,
    }
    file.as_file_mut().flush()?;
    file.as_file().sync_all()?;
    Ok(file)
}

fn export_exists(path: &Path) -> Error {
    Error::Conflict(format!(
        "导出文件已存在，已保留原文件；请打开已有导出或选择其他目录：{}",
        path.display()
    ))
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
    writeln!(
        output,
        "作品简介：\n{}\n",
        if content.description.trim().is_empty() {
            "来源未提供简介"
        } else {
            &content.description
        }
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
    archive.start_file("EPUB/about.xhtml", compressed)?;
    let description = if content.description.trim().is_empty() {
        "来源未提供简介"
    } else {
        &content.description
    };
    write!(
        archive,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" lang=\"zh-CN\" xml:lang=\"zh-CN\"><head><title>作品信息</title></head><body><h1>{title}</h1><p>作者：{}</p><p>来源：{}</p><p>{}</p><p>{}</p><h2>作品简介</h2>{}</body></html>",
        xml(&content.summary.author),
        xml(content.summary.source_label()),
        xml(&content.summary.source_url().unwrap_or_default()),
        xml(&coverage(content)),
        description
            .lines()
            .map(|p| format!("<p>{}</p>", xml(p)))
            .collect::<String>()
    )?;
    manifest
        .push_str("<item id=\"about\" href=\"about.xhtml\" media-type=\"application/xhtml+xml\"/>");
    let mut spine = String::from("<itemref idref=\"about\"/>");
    let mut navigation = String::from("<li><a href=\"about.xhtml\">作品信息与简介</a></li>");
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
    let modified = chrono::NaiveDateTime::parse_from_str(&content.modified, "%Y-%m-%d %H:%M:%S")
        .map_err(|_| Error::Invalid("本地内容时间字段无效".into()))?
        .and_utc()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let identifier = uuid::Uuid::parse_str(
        content
            .summary
            .id
            .strip_prefix("history-")
            .unwrap_or(&content.summary.id),
    )
    .map_err(|_| Error::Invalid("本地电子书标识无效".into()))?;
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
        identifier,
        xml(&format!("{}\n{}", coverage(content), description))
    )?;
    archive.finish()?;
    Ok(())
}
