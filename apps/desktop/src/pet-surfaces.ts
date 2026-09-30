import {climbDistance} from './pet-climb';
/** Desktop-window locomotion. Geometry is monitor-local logical pixels, y grows down.
 * Browser preview counterpart of native_pet/physics.rs; no OS calls or rendering here.
 */
export interface SurfaceWindow { id:string;x:number;y:number;width:number;height:number }
export interface SurfaceWorld { monitor:string;x:number;y:number;width:number;height:number;size:number;windows:SurfaceWindow[] }
export interface Ledge { id:string;left:number;right:number;y:number }
export type Motion='grounded'|'fall'|'land'|'prepare'|'jump'|'grab'|'climb'|'pull'|'wobble';
export interface MotionInput { walk:number;autonomous:boolean;reduced:boolean;paused?:boolean }
const clamp=(v:number,a:number,b:number)=>Math.max(a,Math.min(Math.max(a,b),v));
const smooth=(t:number)=>t*t*(3-2*t);

export function subtractInterval(parts:[number,number][],left:number,right:number):[number,number][] {
  return parts.flatMap(([a,b])=>right<=a||left>=b?[[a,b]]:([[a,Math.min(b,left)],[Math.max(a,right),b]] as [number,number][]).filter(([l,r])=>r>l));
}
/** Front-to-back rectangles. Subtract anything covering the ledge OR the pet above it. */
export function exposedLedges(w:SurfaceWorld):Ledge[] {
  const half=w.size*.46,height=w.size*.83;
  const out:Ledge[]=[];
  w.windows.forEach((win,i)=>{
    if(win.y<height||win.y>w.height)return;
    let parts:[number,number][]=[[Math.max(0,win.x),Math.min(w.width,win.x+win.width)]];
    for(const front of w.windows.slice(0,i)) {
      if(front.y<win.y+1&&front.y+front.height>win.y-height)parts=subtractInterval(parts,front.x,front.x+front.width);
    }
    for(const [left,right] of parts)if(right-left>=half*2)out.push({id:win.id,left,right,y:win.y});
  });
  return out;
}
export class SurfaceMotion {
  x:number;y:number;direction=1;motion:Motion='fall';age=0;
  world:SurfaceWorld;ledges:Ledge[]=[];
  private support:string|null=null;
  private attached:SurfaceWindow|null=null;
  private side:1|-1=1;
  private vx=0;private vy=0;
  private cooldown=4;private plan:{x:number;y:number;vx:number;vy:number;id:string}|null=null;
  private pullStart={x:0,y:0};
  private random:()=>number;
  constructor(world:SurfaceWorld,random=Math.random){this.world=world;this.x=world.x;this.y=world.y;this.random=random;this.updateWorld(world);this.reset(world.x,world.y);}
  get half(){return this.world.size*.46;}
  get height(){return this.world.size*.83;}
  get busy(){return this.motion!=='grounded';}
  get supportId(){return this.support;}
  private enter(m:Motion){this.motion=m;this.age=0;}
  private ledgeAt(x:number,y:number,id?:string){return this.ledges.find(p=>(!id||p.id===id)&&Math.abs(p.y-y)<3&&x>=p.left+this.half&&x<=p.right-this.half);}
  private attach(p:Ledge|null){this.support=p?.id??null;this.attached=this.world.windows.find(w=>w.id===this.support)??null;}
  reset(x:number,y:number){this.x=clamp(x,this.half,this.world.width-this.half);this.y=clamp(y,this.height,this.world.height);this.vx=this.vy=0;this.plan=null;this.attach(this.ledgeAt(this.x,this.y)??null);this.enter(this.support||this.y>=this.world.height-1?'grounded':'fall');}
  updateWorld(world:SurfaceWorld){
    world={...world,windows:world.windows.map(w=>({...w}))};
    if(world.monitor!==this.world.monitor||world.size!==this.world.size){this.world=world;this.ledges=exposedLedges(world);this.attached=null;this.support=null;this.reset(world.x,world.y);return;}
    this.world=world;this.ledges=exposedLedges(world);
    if(this.attached){
      const next=world.windows.find(w=>w.id===this.attached!.id);
      if(next){
        const old=this.attached;
        if(['grab','climb','pull'].includes(this.motion)){
          const dx=(this.side===1?next.x:next.x+next.width)-(this.side===1?old.x:old.x+old.width);
          this.x+=dx;this.y+=next.y-old.y;this.pullStart.x+=dx;this.pullStart.y+=next.y-old.y;
        }else{
          // Follow the same window ID, preserving horizontal relative position on resize.
          this.x=next.x+(this.x-old.x)/old.width*next.width;this.y+=next.y-old.y;
        }
        this.attached=next;
        const climbing=['grab','climb','pull'].includes(this.motion);
        if((!climbing&&!this.ledgeAt(this.x,this.y,next.id))||(climbing&&!this.climbVisible(next))||this.x<this.half||this.x>world.width-this.half||this.y<this.height||this.y>world.height)this.fall();
      }else this.fall();
    }
    this.x=clamp(this.x,this.half,world.width-this.half);this.y=clamp(this.y,this.height,world.height);
  }
  private fall(){this.attach(null);this.plan=null;this.vx=this.vy=0;this.enter('fall');}
  private climbVisible(w:SurfaceWindow){
    const edge=this.side===1?w.x:w.x+w.width;
    if(w.y<this.height||edge<this.half*2||edge>this.world.width-this.half*2)return false;
    const left=this.side===1?edge-this.half*2:edge;
    return !this.world.windows.slice(0,this.world.windows.findIndex(v=>v.id===w.id)).some(v=>v.x<left+this.half*2&&v.x+v.width>left&&v.y<this.y&&v.y+v.height>Math.min(w.y,this.y-this.height));
  }
  private startClimb(dx:number){
    if(!dx)return false;
    const side=dx>0?1:-1;
    for(const w of this.world.windows){
      const edge=side===1?w.x:w.x+w.width;
      const handX=this.x+side*this.half;
      if(this.y<=w.y+this.height*.35||this.y-this.height>=w.y+w.height||Math.abs(edge-handX)>Math.abs(dx)+5||(edge-handX)*side < -3)continue;
      this.side=side;
      const landing=this.ledges.find(p=>p.id===w.id&&(side===1?p.left<=w.x+2:p.right>=w.x+w.width-2));
      if(!landing||!this.climbVisible(w))continue;
      this.direction=side;this.x=edge-side*this.half;this.support=w.id;this.attached=w;this.enter('grab');return true;
    }
    return false;
  }
  private jumpPlan(){
    const g=900,candidates=this.ledges.filter(p=>p.id!==this.support&&Math.abs(p.y-this.y)<220);
    const plans=candidates.flatMap(p=>{
      const targetX=clamp(this.x,p.left+this.half+8,p.right-this.half-8),dx=targetX-this.x;
      if(Math.abs(dx)<this.half||Math.abs(dx)>300)return [];
      const rise=Math.max(70,this.y-p.y+55),apex=this.y-rise;
      if(apex<this.height+5)return [];
      const vy=-Math.sqrt(2*g*rise),time=(-vy+Math.sqrt(vy*vy+2*g*(p.y-this.y)))/g;
      if(time<=0||Math.abs(dx/time)>350)return [];
      // No passing through higher visible ledges on the descending half of the arc.
      return [{x:targetX,y:p.y,vx:dx/time,vy,id:p.id}];
    });
    return plans[Math.floor(this.random()*plans.length)]??null;
  }
  step(seconds:number,input:MotionInput){
    // Substeps prevent tunnelling through narrow ledges even after delayed frames.
    const dt=clamp(seconds,0,.1),n=Math.max(1,Math.ceil(dt/(1/60)));
    for(let i=0;i<n;i++)this.tick(dt/n,input);
    return {x:this.x,y:this.y,motion:this.motion,direction:this.direction,age:this.age};
  }
  get ropeAnchor(){const w=this.attached;return w&&['grab','climb','pull'].includes(this.motion)?{x:this.side===1?w.x:w.x+w.width,y:w.y}:null;}
  private tick(dt:number,input:MotionInput){
    if(input.paused)return;
    const oldAge=this.age;this.age+=dt;this.cooldown=Math.max(0,this.cooldown-dt);
    if(input.reduced){
      // Keep stable surfaces; resolve loss of support without showing a falling animation.
      if(this.motion!=='grounded'){
        const p=this.ledges.filter(p=>p.y>=this.y-3&&this.x>=p.left+this.half&&this.x<=p.right-this.half).sort((a,b)=>a.y-b.y)[0];
        this.y=p?.y??this.world.height;this.attach(p??null);this.enter('grounded');this.vx=this.vy=0;
      }
      return;
    }
    if(this.motion==='land'){if(this.age>=.45)this.enter('grounded');return;}
    if(this.motion==='grab'){
      if(this.age>=.35)this.enter('climb');return;
    }
    if(this.motion==='climb'){
      if(!this.attached){this.fall();return;}
      this.y=Math.max(this.attached.y+this.height*.6,this.y-(climbDistance(this.age,this.world.size)-climbDistance(oldAge,this.world.size)));
      if(this.y<=this.attached.y+this.height*.6+.1){this.pullStart={x:this.x,y:this.y};this.enter('pull');}return;
    }
    if(this.motion==='pull'){
      const w=this.attached;if(!w){this.fall();return;}
      const targetX=this.side===1?w.x+this.half+3:w.x+w.width-this.half-3;
      const t=smooth(Math.min(1,this.age/.7));
      this.x=this.pullStart.x+(targetX-this.pullStart.x)*t;this.y=this.pullStart.y+(w.y-this.pullStart.y)*t;
      if(t>=1){const p=this.ledgeAt(this.x,this.y,w.id);if(p){this.attach(p);this.enter('land');this.cooldown=6;}else this.fall();}return;
    }
    if(this.motion==='prepare'){
      if(!input.autonomous||!this.plan||!this.ledges.some(p=>p.id===this.plan!.id&&Math.abs(p.y-this.plan!.y)<3&&this.plan!.x>=p.left+this.half&&this.plan!.x<=p.right-this.half)){this.plan=null;this.enter('grounded');return;}
      if(this.age>=.28){const p=this.plan;this.attach(null);this.vx=p.vx;this.vy=p.vy;this.direction=Math.sign(this.vx)||1;this.enter('jump');}return;
    }
    if(this.motion==='wobble'){
      if(this.age>=.65){
        // Occasional tumble; most of the time regain balance and turn back.
        if(input.autonomous&&this.support&&this.random()<.18){this.fall();this.y+=3;this.vx=this.direction*100;}
        else {this.direction*=-1;this.enter('grounded');}
        this.cooldown=3;
      }return;
    }
    if(this.motion==='fall'||this.motion==='jump'){
      const oldX=this.x,oldY=this.y;
      this.vy=Math.min(700,this.vy+900*dt);this.x=clamp(this.x+this.vx*dt,this.half,this.world.width-this.half);this.y=Math.max(this.height,this.y+this.vy*dt);
      if(this.y<=this.height&&this.vy<0)this.vy=0;
      if(this.vy>=0){
        const p=this.ledges.filter(p=>{
          if(p.y<oldY-2||p.y>this.y)return false;
          const x=oldX+(this.x-oldX)*clamp((p.y-oldY)/(this.y-oldY||1),0,1);
          return x>=p.left+this.half&&x<=p.right-this.half;
        }).sort((a,b)=>a.y-b.y)[0];
        if(p||this.y>=this.world.height){this.y=p?.y??this.world.height;if(p)this.x=clamp(this.x,p.left+this.half,p.right-this.half);this.attach(p??null);this.vx=this.vy=0;this.plan=null;this.enter('land');this.cooldown=5;}
      }return;
    }
    if(this.support&&!this.ledgeAt(this.x,this.y,this.support)){this.fall();return;}
    if(!this.support&&this.y<this.world.height-1){this.fall();return;}
    if(!input.autonomous)return;
    if(this.cooldown===0){this.cooldown=8+this.random()*8;const plan=this.jumpPlan();if(plan&&this.random()<.55){this.plan=plan;this.enter('prepare');return;}}
    const dx=input.walk*this.direction*dt;
    if(!dx)return;
    if(this.startClimb(dx))return;
    const next=this.x+dx,p=this.support?this.ledgeAt(this.x,this.y,this.support):null;
    const left=p?p.left+this.half:this.half,right=p?p.right-this.half:this.world.width-this.half;
    if(next<left||next>right){this.x=clamp(next,left,right);this.enter('wobble');return;}
    this.x=next;
  }
}
