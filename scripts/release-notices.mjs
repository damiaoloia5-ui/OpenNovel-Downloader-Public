import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const root=path.resolve(import.meta.dirname,'..');
const metadata=JSON.parse(execFileSync('cargo',['metadata','--format-version','1','--locked','--filter-platform','x86_64-pc-windows-msvc'],{cwd:root,maxBuffer:32*1024*1024,encoding:'utf8',stdio:['ignore','pipe','inherit']}));
const blocks=['OpenNovel Downloader third-party license notices\nGenerated from the locked dependency graph. Some listed crates are build/test or other-target dependencies.\nThe project source is Apache-2.0; dependencies retain their original licenses.\nFor crate sources, use Cargo.lock and cargo vendor, or https://crates.io/crates/NAME/VERSION.\n'];
const missing=[];
const missingPackages=[];
const overrides=JSON.parse(fs.readFileSync(path.join(root,'third_party/license-overrides.json'),'utf8'));
const used=new Set(metadata.resolve.nodes.map(node=>node.id));
for(const pkg of metadata.packages.filter(pkg=>used.has(pkg.id)).sort((a,b)=>a.name.localeCompare(b.name)||a.version.localeCompare(b.version))){
  if(!pkg.source) continue;
  const directory=path.dirname(pkg.manifest_path);
  const files=new Set(fs.readdirSync(directory).filter(name=>/^(license|copying|notice)([.-]|$)/i.test(name)&&fs.statSync(path.join(directory,name)).isFile()).map(name=>path.join(directory,name)));
  const licenseFile=pkg.license_file&&path.resolve(directory,pkg.license_file);
  if(licenseFile&&fs.existsSync(licenseFile)) files.add(licenseFile);
  if(!files.size){
    const override=overrides[`${pkg.name}@${pkg.version}`];
    for(const entry of override?.files ?? []){
      const file=path.resolve(root,entry.path);
      if(createHash('sha256').update(fs.readFileSync(file)).digest('hex')!==entry.sha256)throw new Error(`License hash mismatch: ${entry.path}`);
      files.add(file);
    }
    if(override) blocks.push(`License text supplement: ${JSON.stringify(override)}\n`);
  }
  blocks.push(`\n========== Rust: ${pkg.name} ${pkg.version} ==========\nLicense: ${pkg.license ?? 'see files'}\nSource: https://crates.io/crates/${pkg.name}/${pkg.version}\n`);
  if(!files.size) {missing.push(`${pkg.name}@${pkg.version}`);missingPackages.push({name:pkg.name,version:pkg.version,license:pkg.license,repository:pkg.repository,directory});}
  for(const file of files)blocks.push(`\n--- ${path.basename(file)} ---\n${fs.readFileSync(file,'utf8')}\n`);
}
for(const name of ['vue','@vue/shared','@vue/reactivity','@vue/runtime-core','@vue/runtime-dom','@vue/compiler-core','@vue/compiler-dom','@vue/compiler-sfc','@vue/server-renderer','@tauri-apps/api']){
  const directory=path.join(root,'node_modules',name);if(!fs.existsSync(directory))continue;
  const pkg=JSON.parse(fs.readFileSync(path.join(directory,'package.json'),'utf8'));
  blocks.push(`\n========== JavaScript: ${name} ${pkg.version} ==========\nLicense: ${pkg.license}\n`);
  for(const file of fs.readdirSync(directory).filter(n=>/^license/i.test(n)&&fs.statSync(path.join(directory,n)).isFile()))blocks.push(fs.readFileSync(path.join(directory,file),'utf8'));
}
const output=path.join(root,'.artifacts/release');fs.mkdirSync(output,{recursive:true});
fs.writeFileSync(path.join(output,'THIRD_PARTY_NOTICES.txt'),blocks.join('\n'));
fs.writeFileSync(path.join(output,'missing-license-packages.json'),JSON.stringify(missingPackages,null,2));
if(missing.length){console.error('Crates without packaged license text:',missing.join(', '));process.exitCode=1;}
else console.log(`License notices written for ${metadata.packages.length} locked packages.`);
