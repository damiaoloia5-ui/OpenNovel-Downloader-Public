//! Standalone WordprocessingML export; no Office or external runtime is required.
use crate::{BookContent, Result};
use std::io::Write;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn xml(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            matches!(c, '\t' | '\n' | '\r') || (*c >= ' ' && !matches!(*c, '\u{fffe}' | '\u{ffff}'))
        })
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn write_docx(output: &mut std::fs::File, content: &BookContent) -> Result<()> {
    write_docx_with_metadata(output, content, None)
}

pub(crate) fn write_docx_with_metadata(
    output: &mut std::fs::File,
    content: &BookContent,
    metadata: Option<[String; 3]>,
) -> Result<()> {
    let mut archive = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, value) in [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/footer.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="document" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="footer" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer.xml"/></Relationships>"#,
        ),
        ("word/styles.xml", STYLES),
        (
            "word/footer.xml",
            r#"<?xml version="1.0" encoding="UTF-8"?><w:ftr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:pPr><w:jc w:val="center"/><w:ind w:firstLine="0" w:firstLineChars="0"/></w:pPr><w:fldSimple w:instr="PAGE"><w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>1</w:t></w:r></w:fldSimple></w:p></w:ftr>"#,
        ),
    ] {
        archive.start_file(name, options)?;
        archive.write_all(value.as_bytes())?;
    }
    archive.start_file("word/document.xml", options)?;
    archive.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>"#)?;
    paragraph(&mut archive, "Title", &content.summary.title)?;
    paragraph(
        &mut archive,
        "Metadata",
        &format!("作者：{}", content.summary.author),
    )?;
    for value in metadata.unwrap_or_else(|| {
        [
            format!("来源：{}", content.summary.source_label()),
            content.summary.source_url().unwrap_or_default(),
            format!(
                "本地保存 {} / {} 章{}",
                content.summary.saved_chapters,
                content.summary.total_chapters,
                if content.summary.saved_chapters < content.summary.total_chapters {
                    "（部分内容）"
                } else {
                    ""
                }
            ),
        ]
    }) {
        paragraph(&mut archive, "Metadata", &value)?;
    }
    paragraph(&mut archive, "Metadata", "作品简介")?;
    let description = if content.description.trim().is_empty() {
        "来源未提供简介"
    } else {
        &content.description
    };
    for line in description.lines().filter(|s| !s.trim().is_empty()) {
        paragraph(&mut archive, "Metadata", line.trim())?;
    }
    for chapter in &content.chapters {
        paragraph(&mut archive, "Heading1", &chapter.title)?;
        for line in chapter
            .paragraphs
            .iter()
            .flat_map(|p| p.lines())
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            paragraph(&mut archive, "Normal", line)?;
        }
    }
    archive.write_all(br#"<w:sectPr><w:footerReference w:type="default" r:id="footer"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#)?;
    archive.finish()?;
    Ok(())
}

fn paragraph(output: &mut impl Write, style: &str, text: &str) -> Result<()> {
    write!(
        output,
        "<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml(text)
    )?;
    Ok(())
}

const STYLES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="宋体" w:cs="Times New Roman"/><w:sz w:val="24"/><w:szCs w:val="24"/><w:lang w:val="en-US" w:eastAsia="zh-CN"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:line="360" w:lineRule="auto"/><w:widowControl/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="120" w:line="360" w:lineRule="auto"/><w:ind w:firstLine="480" w:firstLineChars="200"/><w:jc w:val="both"/><w:widowControl/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/><w:next w:val="Metadata"/><w:qFormat/><w:pPr><w:ind w:firstLine="0" w:firstLineChars="0"/><w:spacing w:before="720" w:after="360"/><w:jc w:val="center"/><w:keepNext/></w:pPr><w:rPr><w:rFonts w:eastAsia="黑体"/><w:sz w:val="40"/><w:szCs w:val="40"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Metadata"><w:name w:val="作品信息"/><w:basedOn w:val="Normal"/><w:pPr><w:ind w:firstLine="0" w:firstLineChars="0"/><w:jc w:val="left"/></w:pPr><w:rPr><w:rFonts w:eastAsia="楷体"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:pageBreakBefore/><w:keepNext/><w:keepLines/><w:ind w:firstLine="0" w:firstLineChars="0"/><w:spacing w:before="120" w:after="360" w:line="360" w:lineRule="auto"/><w:jc w:val="center"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:eastAsia="黑体"/><w:sz w:val="30"/><w:szCs w:val="30"/></w:rPr></w:style>
</w:styles>"#;
