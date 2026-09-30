import registration from './surface-registration.json';
import type { Motion } from './pet-surfaces';
export function surfaceFrame(motion:Motion,age:number):number {
  switch(motion){
    case 'grab':return 12+Math.min(3,Math.floor(age/.1));
    case 'climb':return Math.floor(age/.16)%4;
    case 'pull':return 4+Math.min(3,Math.floor(age/.175));
    case 'prepare':return 8;
    case 'jump':return age<.15?9:10;
    case 'fall':return 10;
    case 'land':return 11;
    default:return 7;
  }
}
/** Feet registered to the same baseline. Hanging poses align the reaching hand to the wall. */
export function drawSurfacePose(ctx:CanvasRenderingContext2D,image:HTMLImageElement,motion:Motion,age:number,direction:number,x=200,y=250,height=170){
  if(!image.complete||!image.naturalWidth)return;
  const r=registration.frames[surfaceFrame(motion,age)]!,scale=height/registration.referenceHeight;
  let offset=0;
  if(motion==='grab'||motion==='climb')offset=119.6-(r.right-r.left)*scale/2;
  if(motion==='pull')offset=(119.6-(r.right-r.left)*scale/2)*(1-Math.min(1,age/.7));
  ctx.save();ctx.translate(x,y);ctx.scale(direction,1);
  ctx.drawImage(image,r.x,r.y,r.width,r.height,-(r.left+r.right)*scale/2+offset,-r.bottom*scale,r.width*scale,r.height*scale);
  ctx.restore();
}
