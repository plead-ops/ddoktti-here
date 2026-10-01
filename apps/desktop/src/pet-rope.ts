/** Preview of the native rope; the attachment points use the same reviewed artwork. */
import hands from './pet-climb-hands.json';
import {vectorFrame} from './pet-vector';
const NS='http://www.w3.org/2000/svg';
export function renderPetRope(svg:SVGSVGElement,pose:string,elapsed:number,options:{x:number;y:number;scale?:number;direction?:number;reduced?:boolean;anchor:{x:number;y:number}|null}){
 let group=svg.querySelector<SVGGElement>('[data-rope]');
 if(!group){group=svg.ownerDocument.createElementNS(NS,'g');group.dataset.rope='true';group.setAttribute('pointer-events','none');group.innerHTML='<path/><path/><path/>';svg.insertBefore(group,svg.firstChild);}
 const {anchor}=options;
 group.style.display=!anchor||options.reduced||!['grab','climb','pull','lower','descend'].includes(pose)?'none':'';
 if(group.style.display==='none'||!anchor)return;
 const phase=elapsed/1000,retract=pose==='pull'?Math.max(0,Math.min(1,(phase-.3)/.4)):pose==='lower'?Math.max(0,Math.min(1,1-phase/.4)):0;
 const opacity=(1-retract)*(pose==='grab'?Math.min(1,phase/.15):1);
 group.setAttribute('opacity',String(opacity));
 const f=vectorFrame(pose,elapsed),points=hands[String(f.index) as keyof typeof hands];if(!points)return;
 const k=options.scale??1,d=options.direction===-1?-1:1;
 const point=(p:number[])=>{const x=options.x+d*(p[0]!+f.left)*k,y=options.y+(p[1]!+f.top)*k;return {x:x+(anchor.x-x)*retract,y:y+(anchor.y-y)*retract};};
 const h=point(points[0]!),l=point(points[1]!),tail={x:l.x-d*8*k,y:l.y+26*k*(1-retract)};
 const path=`M ${anchor.x} ${anchor.y} L ${h.x} ${h.y} Q ${h.x} ${l.y} ${l.x} ${l.y} Q ${tail.x+8*k} ${tail.y-10*k} ${tail.x} ${tail.y}`;
 const paths=group.querySelectorAll('path');
 for(let i=0;i<3;i++){const p=paths[i]!;p.setAttribute('fill','none');p.setAttribute('stroke-linecap','round');p.setAttribute('stroke-width',String([5,3,4][i]!*k));p.setAttribute('stroke',['#23303b','#ddae5b','#586777'][i]!);p.setAttribute('d',i<2?path:`M ${anchor.x+d*10*k} ${anchor.y-3*k} L ${anchor.x} ${anchor.y-3*k} Q ${anchor.x-d*5*k} ${anchor.y-3*k} ${anchor.x-d*5*k} ${anchor.y+6*k}`);}
}
