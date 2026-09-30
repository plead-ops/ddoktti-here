// Export self-contained static SVG previews from the traced production frames.
// Re-trace only when original artwork changes: see docs/vector-artwork.md.
import fs from 'node:fs/promises';
import ts from 'typescript';
const root=new URL('../',import.meta.url);
const data=JSON.parse(await fs.readFile(new URL('apps/desktop/src/pet-vector-frames.json',root),'utf8'));
const artwork=JSON.parse(await fs.readFile(new URL('apps/desktop/src/pet-vector-paths.json',root),'utf8'));
const behaviorSource=await fs.readFile(new URL('apps/desktop/src/pet-behaviors.ts',root),'utf8');
const source=(await fs.readFile(new URL('apps/desktop/src/pet-vector.ts',root),'utf8'))
 .replace("import {gaitFrame} from './pet-gait';",(await fs.readFile(new URL('apps/desktop/src/pet-gait.ts',root),'utf8')).replace("import gaits from './pet-gaits.json';",'const gaits='+ await fs.readFile(new URL('apps/desktop/src/pet-gaits.json',root),'utf8')+';'))
 .replace("import {climbFrame} from './pet-climb';",await fs.readFile(new URL('apps/desktop/src/pet-climb.ts',root),'utf8'))
 .replace("import artwork from './pet-vector-paths.json';",`const artwork=${JSON.stringify(artwork)};`)
 .replace("import frames from './pet-vector-frames.json';",`const frames=${JSON.stringify(data)};`)
 .replace("import { behaviors, behaviorPose } from './pet-behaviors';",behaviorSource);
const js=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {VECTOR_POSES,vectorPetSvg}=await import('data:text/javascript;base64,'+Buffer.from(js).toString('base64'));
const folder=new URL('apps/desktop/public/sprites/vector/',root);
for(const pose of VECTOR_POSES){
 let svg=vectorPetSvg(pose,pose==='pull'?350:pose==='land'?0:600);
 svg=svg.replace(/<rect data-hit="true"[^>]*\/>/g,'');
 await fs.writeFile(new URL(pose+'.svg',folder),svg+'\n');
}
console.log(`Exported ${VECTOR_POSES.length} faithful SVG pose previews`);
