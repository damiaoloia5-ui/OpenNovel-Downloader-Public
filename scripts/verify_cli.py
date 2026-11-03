"""Native CLI original-content round trip in a new directory, without platform requests."""
import argparse
import json
import pathlib
import subprocess
import zipfile
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--executable', type=pathlib.Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if not args.executable.is_absolute() or not args.output.is_absolute() or args.output.exists():
        parser.error('absolute executable and new absolute output directory required')
    root = args.output
    root.mkdir(parents=True)
    exe = str(args.executable)
    version = subprocess.check_output([exe, '--version'], text=True).strip()
    if version != 'opennovel-cli ' + args.version:
        raise ValueError('CLI version does not match this source build')
    database = root / 'original-library.db'

    def command(*values):
        result = subprocess.run([exe, '--database', str(database), *values],
                                text=True, encoding='utf-8', capture_output=True, timeout=30)
        if result.returncode:
            raise RuntimeError('original CLI operation failed; no user library was opened')
        return json.loads(result.stdout)

    repo = pathlib.Path(__file__).resolve().parents[1]
    job = command('import', str(repo / 'examples/local-book.json'))
    if job['state'] != 'completed' or job['saved'] != 2 or job['total'] != 2:
        raise ValueError('original CLI import incomplete')
    receipts = {}
    for fmt in ['txt', 'epub', 'docx']:
        receipt = command('export', job['book_id'], '--format', fmt, '--output', str(root / 'exports'))
        if receipt['partial'] or receipt['saved_chapters'] != 2 or receipt['total_chapters'] != 2:
            raise ValueError('original CLI export incomplete')
        receipts[fmt] = receipt
    text = pathlib.Path(receipts['txt']['path']).read_text(encoding='utf-8')
    if '清晨的阳光落在书架上' not in text or '雨停之后' not in text:
        raise ValueError('original CLI TXT body mismatch')
    with zipfile.ZipFile(receipts['epub']['path']) as archive:
        if archive.testzip() is not None or archive.read('mimetype') != b'application/epub+zip':
            raise ValueError('original CLI EPUB integrity mismatch')
        if not {'EPUB/package.opf', 'EPUB/nav.xhtml', 'META-INF/container.xml'} <= set(archive.namelist()):
            raise ValueError('original CLI EPUB structure missing')
    with zipfile.ZipFile(receipts['docx']['path']) as archive:
        if archive.testzip() is not None:
            raise ValueError('original CLI Word integrity mismatch')
        for name in archive.namelist():
            ET.fromstring(archive.read(name))
        document = archive.read('word/document.xml').decode('utf-8')
        if '清晨的阳光落在书架上' not in document or '雨停之后' not in document:
            raise ValueError('original CLI Word body mismatch')
    report = {'version': args.version, 'passed': True, 'original_chapters': 2,
              'docx_zip_xml_body_checked': True,
              'txt_body_checked': True, 'epub_zip_checked': True, 'receipts': receipts,
              'real_platform_requested': False, 'user_library_opened': False,
              'epubcheck_executed': False}
    (root / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'passed': True, 'original_chapters': 2}), flush=True)


if __name__ == '__main__':
    main()
