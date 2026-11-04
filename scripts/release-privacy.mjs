// Fail closed on additional bundle resources; never scan or copy a user's library.
import {readFileSync,readdirSync,lstatSync,existsSync,mkdirSync,writeFileSync} from 'node:fs';
import {resolve,join,relative,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';

const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const json=p=>JSON.parse(readFileSync(p,'utf8').replace(/^\uFEFF/,''));
const digest=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
export function inventory(root){
  const files=[];
  function walk(dir){for(const name of readdirSync(dir)){const p=join(dir,name),s=lstatSync(p);if(s.isSymbolicLink())throw Error('Distribution contains a link: '+relative(root,p));if(s.isDirectory())walk(p);else if(s.isFile())files.push(relative(root,p).replaceAll('\\','/'));else throw Error('Unexpected file type');}}
  if(lstatSync(root).isSymbolicLink())throw Error('Distribution root is a link');
  walk(root);return files.sort();
}
export function verifyExact(root,expected){
  assert.deepEqual(inventory(root),Object.keys(expected).sort(),'Unexpected or missing distribution files; personal data must not be bundled');
  for(const [name,hash] of Object.entries(expected))assert.equal(digest(join(root,name)),hash,'Resource content differs: '+name);
  return Object.keys(expected).length;
}
export function verifyConfig(root=repo){
  const version=json(join(root,'package.json')).version;
  const path=join(root,'apps/desktop/src-tauri');
  const basic={'../../../LICENSE':'LICENSE','../../../scripts/windows-installation-check.cmd':'运行安装自检.cmd','../../../.artifacts/release/THIRD_PARTY_NOTICES.txt':'THIRD_PARTY_NOTICES.txt'};
  const base=json(join(path,'tauri.conf.json'));
  for(const name of ['tauri.conf.json','tauri.installer.conf.json','tauri.integrated.conf.json']){
    const config=json(join(path,name));
    assert.equal(config.bundle.externalBin,undefined,'Additional external payloads require privacy review');
    assert.equal(config.bundle.windows?.nsis?.template,undefined,'Custom installer template requires privacy review');
    assert.equal(config.bundle.windows?.nsis?.installerHooks,undefined,'Custom installer hooks require privacy review');
  }
  assert.deepEqual(base.bundle.resources,{'../../../LICENSE':'LICENSE'});
  assert.equal(base.bundle.windows.nsis.installMode,'currentUser');
  assert.deepEqual(json(join(path,'tauri.installer.conf.json')).bundle.resources,basic);
  assert.deepEqual(json(join(path,'tauri.integrated.conf.json')).bundle.resources,{...basic,[`../../../.artifacts/integrated-resources-${version}/opennovel-engine/`]:'opennovel-engine/'});
  return version;
}
export function engineInventory(root=repo){
  const version=json(join(root,'package.json')).version;
  const component=join(root,`.artifacts/integrated-resources-${version}/opennovel-engine`);
  const sourceLock=join(root,'third_party/local-engine-source-lock.json'),source=json(sourceLock);
  const python=json(join(root,'third_party/python-embed-lock.json'));
  const lock=join(root,'scripts/research/native_engine_lock.json'),engine=json(lock);
  const runtime=['desktop_engine_worker.py','engine_runtime_config.py','persistent_engine.py','persistent_read_gate.py','relay_read_gate.py','https_read_gate.py','local_engine_gate.py','offline_engine_bundle.py','engine_bundle_lock.json','native_engine.py','native_engine_lock.json','windows_engine_sandbox.py'];
  const hashes=Object.fromEntries(runtime.map(n=>[n,digest(join(root,'scripts/research',n))]));
  const metadata={schema_version:1,candidate_commit:source.commit,runtime_sha256:hashes,inventory_sha256:digest(sourceLock),python:'python/python.exe',engine_binary_bundled:true,runtime_ready_checked:false,python_bundled:true,python_version:python.version,engine_binary_license:'GPL-3.0',corresponding_source:'native-engine/corresponding-source.tar.gz'};
  assert.equal(source.commit,engine.candidate_commit);
  assert.deepEqual(json(join(component,'component.json')),metadata,'Component metadata contains unexpected configuration');
  const expected={'inventory.json':digest(sourceLock),'OPENNOVEL-LICENSE.txt':digest(join(root,'LICENSE')),'component.json':digest(join(component,'component.json')),'native-engine/manifest.json':digest(lock)};
  for(const [name,hash] of Object.entries(hashes))expected['runtime/'+name]=hash;
  for(const [name,hash] of Object.entries(python.files))expected['python/'+name]=hash;
  for(const entry of source.files)expected['source/'+entry.path]=entry.sha256;
  for(const [name,entry] of Object.entries(engine.files))expected['native-engine/'+name]=entry.sha256;
  verifyExact(component,expected);
  return {component,expected};
}
export const portableFiles={
  'OpenNovel.exe':'target/release/opennovel-desktop.exe',
  'cli/opennovel.exe':'target/release/opennovel.exe',
  'LICENSE':'LICENSE',
  'THIRD_PARTY_NOTICES.txt':'.artifacts/release/THIRD_PARTY_NOTICES.txt',
  '运行安装自检.cmd':'scripts/windows-installation-check.cmd',
  '使用说明.md':'docs/local-users.md',
  'docs/chapter-word-saving.md':'docs/chapter-word-saving.md',
};
export function verifyPortable(directory,root=repo){
  const {expected}=engineInventory(root);
  const files=Object.fromEntries(Object.entries(portableFiles).map(([name,path])=>[name,digest(join(root,path))]));
  for(const [name,hash] of Object.entries(expected))files['opennovel-engine/'+name]=hash;
  return verifyExact(directory,files);
}
export function verifyInstaller(root=repo){
  const {component,expected}=engineInventory(root);
  const version=verifyConfig(root);
  const source=readFileSync(join(root,'target/release/nsis/x64/installer.nsi'),'utf8');
  const lines=source.split(/\r?\n/).map(s=>s.trim()).filter(s=>s.startsWith('File '));
  const fixed=['File "/oname=$TEMP\\MicrosoftEdgeWebview2Setup.exe" "${WEBVIEW2BOOTSTRAPPERPATH}"','File "/oname=$TEMP\\MicrosoftEdgeWebView2RuntimeInstaller.exe" "${WEBVIEW2INSTALLERPATH}"','File "${MAINBINARYSRCPATH}"'];
  const allowed={'LICENSE':join(root,'LICENSE'),'THIRD_PARTY_NOTICES.txt':join(root,'.artifacts/release/THIRD_PARTY_NOTICES.txt'),'运行安装自检.cmd':join(root,'scripts/windows-installation-check.cmd')};
  for(const name of Object.keys(expected))allowed['opennovel-engine/'+name]=join(component,name);
  const seen=[];
  for(const line of lines){
    if(fixed.includes(line))continue;
    const match=/^File \/a "\/oname=([^"]+)" "([^"]+)"$/.exec(line);
    assert.ok(match,'Unexpected NSIS file instruction');
    const name=match[1].replaceAll('\\','/');assert.ok(allowed[name],'Unexpected installer payload: '+name);
    assert.equal(resolve(match[2]),resolve(allowed[name]),'Installer source outside whitelist');seen.push(name);
  }
  assert.deepEqual(seen.sort(),Object.keys(allowed).sort(),'Installer resource set differs');
  assert.equal(lines.length,Object.keys(allowed).length+fixed.length);
  assert.ok(source.includes('!define INSTALLMODE "currentUser"'));
  return {installer_manifest_checked:true,installer_payload_files:seen.length+2,installer_sha256:digest(join(root,`target/release/bundle/nsis/OpenNovel Downloader_${version}_x64-setup.exe`)),installer_archive_extracted:false};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const version=verifyConfig();let files=0;
  if(process.argv.includes('--integrated'))files=Object.keys(engineInventory().expected).length;
  const index=process.argv.indexOf('--portable');
  if(index!==-1)files=verifyPortable(resolve(process.argv[index+1]));
  const installer=process.argv.includes('--installer')?verifyInstaller():{};
  const report={passed:true,version,checked_resource_files:files,resource_allowlist_checked:true,private_library_included:false,personal_credentials_included:false,...installer};
  const output=join(repo,'.artifacts/release');if(!existsSync(output))mkdirSync(output,{recursive:true});
  writeFileSync(join(output,'privacy-check.json'),JSON.stringify(report,null,2));
  console.log(JSON.stringify(report));
}
