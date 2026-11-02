"""Explicit live compatibility sampling; never run automatically in ordinary CI.

Uses disposable libraries and reviewed public URLs. Does not log in, solve
verification, purchase chapters, or imply that a sample proves full-book support.
"""
import argparse
import concurrent.futures
from contextlib import closing
import datetime
import hashlib
import json
import pathlib
import sqlite3
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile


SAMPLES = {
    'fanqie-web': ['https://fanqienovel.com/page/7400799554502675518',
                   'https://fanqienovel.com/page/7689165288515849240'],
    'qimao-web': ['https://www.qimao.com/shuku/13030701/', 'https://www.qimao.com/shuku/222767/'],
    'jinjiang-web': ['https://www.jjwxc.net/onebook.php?novelid=3200611',
                    'https://www.jjwxc.net/onebook.php?novelid=4472959'],
    'zongheng-web': ['https://www.zongheng.com/detail/1479797', 'https://www.zongheng.com/detail/1510951'],
    '17k-web': ['https://www.17k.com/book/3583700.html', 'https://www.17k.com/book/3631088.html'],
    'shuqi-web': ['https://www.shuqi.com/book/9147568.html', 'https://www.shuqi.com/book/9127028.html'],
}
SESSION_ENTRIES = {
    'qidian': ['https://my.qidian.com/bookcase/', 'https://www.qidian.com/book/1045749425/'],
    'shuqi': ['https://www.shuqi.com/'],
    'ciweimao': ['https://www.ciweimao.com/signup/login', 'https://www.ciweimao.com/book/100450116'],
    'faloo': ['https://u.faloo.com/regist/login.aspx', 'https://b.faloo.com/1484744.html'],
    'migu': ['https://passport.migu.cn/login?sourceid=204001', 'https://www.cmread.com/yuekan/novel/600000401811.html'],
}


def select_chapters(chapters):
    opened = [i + 1 for i, c in enumerate(chapters) if not c['restricted']]
    blocked = [i + 1 for i, c in enumerate(chapters) if c['restricted']]
    chosen = [opened[0], opened[len(opened) // 2], opened[-1]] if opened else []
    return sorted(set(chosen + blocked[:1]))


def validate_job(code, job, catalog, chosen):
    expected_blocked = sum(catalog['chapters'][i - 1]['restricted'] for i in chosen)
    expected_saved = len(chosen) - expected_blocked
    if not isinstance(job, dict) or (
        code != (2 if expected_blocked else 0)
        or job.get('saved') != expected_saved or job.get('blocked') != expected_blocked
        or job.get('failed') != 0 or job.get('total') != len(chosen)
    ):
        raise ValueError('saved/blocked/failed counts or process exit differ from the selected catalogue')
    return expected_saved


def run_cli(executable, folder, name, *args):
    completed = subprocess.run([str(executable), '--database', str(folder / 'library.db'), *args],
                               capture_output=True, encoding='utf-8', errors='replace', timeout=120)
    (folder / (name + '.stdout.json')).write_text(completed.stdout, encoding='utf-8')
    (folder / (name + '.stderr.log')).write_text(completed.stderr, encoding='utf-8')
    try:
        data = json.loads(completed.stdout)
    except json.JSONDecodeError:
        data = None
    return completed.returncode, data


def verify_export(receipt, folder, saved, total, fmt):
    if (not isinstance(receipt, dict) or receipt.get('saved_chapters') != saved
            or receipt.get('total_chapters') != total or receipt.get('partial') != (saved < total)):
        raise ValueError('export receipt coverage mismatch')
    path = pathlib.Path(receipt['path']).resolve()
    if not path.is_relative_to(folder.resolve() / 'exports') or not path.is_file():
        raise ValueError('export path outside disposable output')
    if fmt == 'epub':
        with zipfile.ZipFile(path) as archive:
            if archive.testzip() or archive.read('mimetype') != b'application/epub+zip':
                raise ValueError('EPUB integrity mismatch')
    else:
        body = path.read_text(encoding='utf-8')
        # Read the isolated library, never the user's database. Compare all saved
        # paragraphs rather than accepting a successful process or a nonempty file.
        with closing(sqlite3.connect((folder / 'library.db').resolve().as_uri() + '?mode=ro', uri=True)) as db:
            rows = db.execute('SELECT paragraphs FROM chapters').fetchall()
        if len(rows) != saved or any(p not in body for row in rows for p in json.loads(row[0])):
            raise ValueError('TXT does not contain every saved paragraph')
    return {'bytes': path.stat().st_size, 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


def check_source(source, urls, executable, output):
    results = []
    stop = False
    for index, url in enumerate(urls):
        row = {'source': source, 'url': url, 'passed': False}
        if stop:
            row['error'] = 'not requested after an earlier unexpected source failure'
            results.append(row)
            continue
        folder = output / f'{source}-{index}'
        folder.mkdir()
        started = time.monotonic()
        try:
            code, catalog = run_cli(executable, folder, 'inspect', 'inspect', url)
            if code or not isinstance(catalog, dict) or catalog.get('source') != source:
                raise ValueError('catalogue inspect failed; see per-source diagnostic log')
            chapters = catalog['chapters']
            if not chapters:
                raise ValueError('empty catalogue')
            row.update(total=len(chapters), open=sum(not c['restricted'] for c in chapters),
                       restricted=sum(c['restricted'] for c in chapters), description=bool(catalog['description']))
            if source == 'shuqi-web':
                code, _ = run_cli(executable, folder, 'disabled', 'download', url, '--from', '1', '--to', '1')
                # Refusal alone is not enough: the documented boundary must be the reason.
                error = (folder / 'disabled.stderr.log').read_text(encoding='utf-8')
                if code == 0 or '书旗' not in error or '正文' not in error:
                    raise ValueError('catalogue-only guard did not produce its expected refusal')
                row['scope'] = 'catalogue and disabled-body guard only'
            else:
                chosen = select_chapters(chapters)
                code, job = run_cli(executable, folder, 'download', 'download', url,
                                    '--chapters', ','.join(map(str, chosen)))
                saved = validate_job(code, job, catalog, chosen)
                row.update(selected=chosen, saved=saved, blocked=job['blocked'], failed=job['failed'])
                if not saved:
                    raise ValueError('no open chapter was available to validate body parsing')
                row['exports'] = {}
                for fmt in ['txt', 'epub']:
                    code, receipt = run_cli(executable, folder, 'export-' + fmt, 'export', job['book_id'],
                                            '--format', fmt, '--output', str(folder / 'exports'))
                    if code:
                        raise ValueError(fmt + ' export failed')
                    row['exports'][fmt] = verify_export(receipt, folder, saved, len(chapters), fmt)
                row['scope'] = 'first/middle/last open chapter and one restricted marker where present'
            row['passed'] = True
        except (OSError, ValueError, KeyError, TypeError, sqlite3.Error, subprocess.SubprocessError, zipfile.BadZipFile) as error:
            row['error'] = str(error)
            stop = True  # Never hammer a source after a challenge, timeout or structure change.
        row['seconds'] = round(time.monotonic() - started, 2)
        results.append(row)
        print(json.dumps(row, ensure_ascii=True), flush=True)
    return results


class OfficialRedirects(urllib.request.HTTPRedirectHandler):
    def __init__(self, hosts):
        self.hosts = hosts

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urllib.parse.urlsplit(newurl)
        if target.scheme != 'https' or target.hostname not in self.hosts or target.username or target.password or target.port:
            raise ValueError('redirect left reviewed HTTPS official hosts')
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def entry_status(status, url):
    if status in (202, 403, 429) or '/man_machine_verify' in urllib.parse.urlsplit(url).path:
        return 'verification-or-rate-limit; browser/account acceptance not proven'
    return 'entry reachable; browser/account acceptance not proven' if status == 200 else 'unexpected-http-status'


def check_entries():
    rows = []
    for source, urls in SESSION_ENTRIES.items():
        hosts = {urllib.parse.urlsplit(url).hostname for url in urls}
        opener = urllib.request.build_opener(OfficialRedirects(hosts))
        for url in urls:
            row = {'source': source, 'url': url, 'account_entitlement_verified': False}
            try:
                request = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 OpenNovel-CompatibilityCheck'})
                try:
                    response = opener.open(request, timeout=25)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    row['http_status'] = response.code
                    row['status'] = entry_status(response.code, response.url)
                    # Deliberately do not store page bodies, cookies or response headers.
            except (OSError, ValueError) as error:
                row['status'] = 'request-failed'
                row['error'] = str(error)
            rows.append(row)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live', action='store_true', help='explicitly allow reviewed official public requests')
    parser.add_argument('--executable', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    args = parser.parse_args()
    if not args.live:
        parser.error('--live is required; ordinary CI must use offline regression tests')
    if (not args.executable.is_absolute() or not args.executable.is_file()
            or not args.output.is_absolute() or args.output.exists()):
        parser.error('existing absolute executable and new absolute output directory required')
    args.output.mkdir(parents=True)
    version = subprocess.check_output([str(args.executable), '--version'], text=True, timeout=10).strip()
    report = {'checked_at': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'cli_version': version,
              'real_platform_requested': True, 'user_library_opened': False,
              'full_book_verified': False, 'account_entitlement_verified': False,
              'local_engine': 'separate native acceptance required; not covered by web CLI'}
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        futures = [pool.submit(check_source, source, urls, args.executable, args.output) for source, urls in SAMPLES.items()]
        report['public_samples'] = [row for future in futures for row in future.result()]
    report['session_entries'] = check_entries()
    report['public_checks_passed'] = all(row['passed'] for row in report['public_samples'])
    report['entry_checks_completed'] = all(row['status'] not in ('request-failed', 'unexpected-http-status') for row in report['session_entries'])
    (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    return 0 if report['public_checks_passed'] and report['entry_checks_completed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
