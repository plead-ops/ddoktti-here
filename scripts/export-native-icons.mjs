// Native platforms require PNG/ICO/ICNS. All of them are derived from the SVG master.
import {execFileSync} from 'node:child_process';
import {mkdtemp, readdir, copyFile, mkdir} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const dir=await mkdtemp(join(tmpdir(),'ddoktti-vector-icons-'));
const pnpm=process.platform==='win32'?'pnpm.cmd':'pnpm';
const run=(...args)=>execFileSync(pnpm,['--filter','@ddoktti/desktop','tauri','icon',resolve(root,'assets/icons/app-icon.svg'),'--output',dir,...args],{cwd:root,stdio:'inherit',shell:process.platform==='win32'});
run();
const dest=resolve(root,'apps/desktop/src-tauri/icons');
for(const name of await readdir(dir))if(/\.(png|ico|icns)$/.test(name))await copyFile(join(dir,name),join(dest,name));
run('--png','16','--png','32');
for(const base of ['apps/desktop/src-tauri/icons/tray','assets/icons/tray']){
 const target=resolve(root,base);await mkdir(target,{recursive:true});
 await copyFile(join(dir,'16x16.png'),join(target,'icon-win.png'));
 await copyFile(join(dir,'32x32.png'),join(target,'icon-win@2x.png'));
}
