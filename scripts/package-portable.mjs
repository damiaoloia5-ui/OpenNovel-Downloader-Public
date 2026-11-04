import {mkdirSync,copyFileSync,writeFileSync} from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';
import {engineInventory,portableFiles,verifyConfig,verifyPortable} from './release-privacy.mjs';
const repo=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const target=process.argv[2];if(!target)throw Error('Provide a new output directory');
const directory=resolve(target);const version=verifyConfig(repo),{component,expected}=engineInventory(repo);
// No overwrite, recursive source copying, external shortcuts, databases or runtime output.
mkdirSync(directory,{recursive:false});
for(const [name,source] of Object.entries(portableFiles)){mkdirSync(dirname(join(directory,name)),{recursive:true});copyFileSync(join(repo,source),join(directory,name));}
for(const name of Object.keys(expected)){mkdirSync(dirname(join(directory,'opennovel-engine',name)),{recursive:true});copyFileSync(join(component,name),join(directory,'opennovel-engine',name));}
const count=verifyPortable(directory,repo);
const archived=spawnSync('python',[join(repo,'scripts/archive_portable.py'),directory],{encoding:'utf8',windowsHide:true});
if(archived.status!==0)throw Error('Portable ZIP verification failed: '+archived.stderr);
verifyPortable(directory,repo);
writeFileSync(directory+'.privacy.json',JSON.stringify({passed:true,version,files:count,private_library_included:false},null,2));
console.log(archived.stdout);
console.log(JSON.stringify({passed:true,directory,files:count}));
