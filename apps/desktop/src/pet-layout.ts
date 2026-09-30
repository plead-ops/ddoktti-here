export function popupPosition(x:number,y:number,petSize:number,width:number,height:number,viewportWidth:number,viewportHeight:number){
 const left=Math.max(8,Math.min(viewportWidth-width-8,x-width/2));
 const above=y-petSize*.83-height-14;
 const below=above<8;
 const top=Math.max(8,Math.min(viewportHeight-height-8,below?y+12:above));
 return {left,top,below,tail:Math.max(20,Math.min(width-20,x-left))};
}
export function layoutPet(doc:Document,anchor:{x:number;y:number},size:number){
 const root=doc.getElementById('pet-root')!;const win=doc.defaultView!;
 root.style.setProperty('--pet-x',anchor.x+'px');root.style.setProperty('--pet-y',(anchor.y+size*10/260)+'px');root.style.setProperty('--pet-size',size+'px');
 for(const id of ['bubble','pet-menu','pet-reaction']){const el=doc.getElementById(id);if(!el)continue;const width=Math.min(id==='bubble'?360:id==='pet-reaction'?260:250,win.innerWidth-16);el.style.width=width+'px';const p=popupPosition(anchor.x,anchor.y,size,width,el.offsetHeight||190,win.innerWidth,win.innerHeight);el.style.left=p.left+'px';el.style.top=p.top+'px';el.classList.toggle('below',p.below);el.style.setProperty('--tail-x',p.tail+'px');}
}
