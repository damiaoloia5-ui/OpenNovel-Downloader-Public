"""Explicit full-catalog benchmarks for the twelve user-selected official books.

Fresh libraries only. A partial download or a reachable entry is never a full-book pass.
Native Fanqie must be measured separately with the ignored desktop benchmark.
"""
import argparse
from verify_sources import OfficialRedirects
import concurrent.futures, datetime, hashlib, json, pathlib, re, sqlite3, subprocess, time, urllib.request
from contextlib import closing
R = pathlib.Path.cwd()
ROOT = None
DEADLINE = 180
EXE = R / 'target/release/opennovel.exe'
SAMPLES = {
 'shuqi': ['https://www.shuqi.com/book/157219.html', 'https://www.shuqi.com/book/8631005.html'],
 'qimao': ['https://www.qimao.com/shuku/1655407/', 'https://www.qimao.com/shuku/215243/'],
 'fanqie': ['https://fanqienovel.com/page/6982529841564224526', 'https://fanqienovel.com/page/7644040936980220953'],
 'zongheng': ['https://www.zongheng.com/detail/916691', 'https://www.zongheng.com/detail/1412188'],
 'ciweimao': ['https://www.ciweimao.com/book/100460510', 'https://www.ciweimao.com/book/100506117'],
 'faloo': ['https://b.faloo.com/1538594.html', 'https://b.faloo.com/1535785.html'],
}
def cli(folder, name, args, timeout=60):
    start = time.monotonic()
    try:
        p = subprocess.run([str(EXE), '--database', str(folder/'library.db'), *args], capture_output=True, timeout=max(.1, timeout))
        code, out, err = p.returncode, p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        code, out, err = 124, e.stdout or b'', e.stderr or b''
    (folder/(name+'.stdout.json')).write_bytes(out)
    (folder/(name+'.stderr.log')).write_bytes(err)
    try: data = json.loads(out)
    except (ValueError, UnicodeError): data = None
    return code, data, err.decode('utf-8', errors='replace').strip(), round(time.monotonic()-start, 3)
def verify_txt(text, rows, receipt, total):
    if (receipt.get('saved_chapters') != len(rows) or receipt.get('total_chapters') != total
            or receipt.get('partial') != (len(rows) < total)):
        return False
    cursor = 0
    for row in rows:
        for paragraph in json.loads(row[0]):
            position = text.find(paragraph, cursor)
            if position < 0:
                return False
            cursor = position + len(paragraph)
    return bool(rows)

def run(source, urls):
    rows=[]
    for index,url in enumerate(urls):
        folder=ROOT/f'{source}-{index+1}';folder.mkdir()
        started=time.monotonic()
        row={'platform':source,'url':url,'full_book_verified':False,'within_180_seconds':False,'saved':0,'account_used':False}
        try:
            code,catalog,error,elapsed=cli(folder,'inspect',['inspect',url])
            row.update(inspect_seconds=elapsed, inspect_exit=code)
            if code or not catalog:
                row['error']=error or 'catalog unavailable'
                if source in ['ciweimao','faloo']:
                    entry_started=time.monotonic()
                    try:
                        req=urllib.request.Request(url,headers={'User-Agent':'Mozilla/5.0 OpenNovel-CompatibilityCheck'})
                        opener = urllib.request.build_opener(OfficialRedirects({'www.ciweimao.com'} if source == 'ciweimao' else {'b.faloo.com'}))
                        with opener.open(req,timeout=20) as resp:
                            raw=resp.read(2*1024*1024)
                            (folder/'entry.html').write_bytes(raw)
                            text=raw.decode('utf-8',errors='replace')
                            if text.count('\ufffd')>5:text=raw.decode('gb18030',errors='replace')
                            title=re.search(r'<title[^>]*>(.*?)</title>',text,re.S)
                            row.update(http_status=resp.status,entry_url=resp.url,page_title=title.group(1).strip() if title else '')
                    except Exception as e:row['entry_error']=str(e)
                    row['entry_seconds']=round(time.monotonic()-entry_started,3)
            else:
                chapters=catalog['chapters']
                row.update(title=catalog['title'],author=catalog['author'],total=len(chapters),catalog_open=sum(not x['restricted'] for x in chapters),catalog_restricted=sum(x['restricted'] for x in chapters))
                remaining=DEADLINE-(time.monotonic()-started)
                code,job,error,elapsed=cli(folder,'download',['download',url],remaining)
                row.update(download_seconds=elapsed,download_exit=code,deadline_reached=code==124,error=error or None)
                with closing(sqlite3.connect((folder/'library.db').as_uri()+'?mode=ro',uri=True)) as db:
                    books=db.execute('SELECT id,total FROM books').fetchall()
                    texts=db.execute('SELECT paragraphs FROM chapters ORDER BY position').fetchall()
                    row['saved']=len(texts)
                    row['saved_characters']=sum(sum(len(''.join(p.split())) for p in json.loads(x[0])) for x in texts)
                    row['states']=dict(db.execute('SELECT state,COUNT(*) FROM job_items GROUP BY state').fetchall())
                    row['first_errors']=[dict(zip(['state','kind','error'],x)) for x in db.execute("SELECT state,error_kind,error FROM job_items WHERE error IS NOT NULL LIMIT 3").fetchall()]
                if books and texts:
                    code,receipt,error,elapsed=cli(folder,'export',['export',books[0][0],'--format','txt','--output',str(folder/'exports')])
                    row['export_seconds']=elapsed
                    if not code and receipt:
                        path=pathlib.Path(receipt['path']);text=path.read_text(encoding='utf-8')
                        row['txt_verified']=verify_txt(text, texts, receipt, len(chapters))
                        row['txt_bytes']=path.stat().st_size
                        row['full_book_verified']=(row['saved']==len(chapters) and row['txt_verified'] and receipt['partial'] is False)
                    else:row['export_error']=error
        except Exception as e:row['error']=str(e)
        row['wall_seconds']=round(time.monotonic()-started,3)
        row['within_180_seconds']=row['full_book_verified'] and row['wall_seconds']<=180
        (folder/'result.json').write_text(json.dumps(row,ensure_ascii=False,indent=2),encoding='utf-8')
        print(json.dumps(row,ensure_ascii=True),flush=True);rows.append(row)
    return rows
def main():
    global ROOT, EXE, DEADLINE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--live', action='store_true')
    parser.add_argument('--executable', type=pathlib.Path, required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--deadline-seconds', type=int, default=180)
    args = parser.parse_args()
    if not args.live:
        parser.error('--live required; ordinary CI must use offline tests')
    if not args.executable.is_absolute() or not args.executable.is_file() or not args.output.is_absolute() or args.output.exists() or not 10 <= args.deadline_seconds <= 3600:
        parser.error('existing absolute executable, new absolute output, budget 10..3600 required')
    ROOT, EXE, DEADLINE = args.output, args.executable, args.deadline_seconds
    ROOT.mkdir(parents=True,exist_ok=False)
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        futures=[pool.submit(run,s,urls) for s,urls in SAMPLES.items()]
        rows=[row for future in futures for row in future.result()]
    (ROOT/'report.json').write_text(json.dumps({'tested_at':datetime.datetime.now().isoformat(),'version':subprocess.check_output([str(EXE),'--version']).decode().strip(),'scope':'fresh isolated web libraries; native Fanqie separately tested','results':rows},ensure_ascii=False,indent=2),encoding='utf-8')

    return 0 if all(x['within_180_seconds'] for x in rows) else 2

if __name__ == '__main__':
    raise SystemExit(main())
