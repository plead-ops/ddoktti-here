import {climbDistance} from './pet-climb';
/** Desktop-window locomotion. Geometry is monitor-local logical pixels, y grows down.
 * Browser preview counterpart of native_pet/physics.rs; no OS calls or rendering here.
 */
export interface SurfaceWindow { id:string;x:number;y:number;width:number;height:number }
export interface SurfaceWorld { monitor:string;x:number;y:number;width:number;height:number;size:number;windows:SurfaceWindow[] }
export interface Ledge { id:string;left:number;right:number;y:number }
export type Motion='grounded'|'fall'|'land'|'prepare'|'jump'|'grab'|'climb'|'pull'|'lower'|'descend'|'wobble'|'hurt';
export interface MotionInput { walk:number;autonomous:boolean;reduced:boolean;paused?:boolean }
const clamp=(v:number,a:number,b:number)=>Math.max(a,Math.min(Math.max(a,b),v));
const smooth=(t:number)=>t*t*(3-2*t);

export function subtractInterval(parts:[number,number][],left:number,right:number):[number,number][] {
  return parts.flatMap(([a,b])=>right<=a||left>=b?[[a,b]]:([[a,Math.min(b,left)],[Math.max(a,right),b]] as [number,number][]).filter(([l,r])=>r>l));
}
/** Front-to-back rectangles. Subtract anything covering the ledge OR the pet above it. */
export function exposedLedges(w:SurfaceWorld):Ledge[] {
  const half=w.size*.07,height=w.size*.83;
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
  private fallOrigin=0;private deliberateJump=false;
  private vx=0;private vy=0;
  private cooldown=4;private plan:{x:number;y:number;vx:number;vy:number;id:string|null}|null=null;
  /** Side of our own support whose corner we walk to before roping down; seconds perched on windows. */
  private descent:1|-1|null=null;private corner:1|-1|null=null;perch=0;
  /** Why the last autonomous choice went the way it did; seconds restless with no way down. */
  lastChoice='';private stuck=0;static readonly STUCK_AFTER=45;
  /** Where we have been lately (-1 left … +1 right, ~10 min average); strolls from the floor lean the other way. */
  balance=0;
  private pullStart={x:0,y:0};
  /** Just pulled onto a window top; the caller shows a proud moment. */
  climbed=false;
  private random:()=>number;
  constructor(world:SurfaceWorld,random=Math.random){this.world=world;this.x=world.x;this.y=world.y;this.random=random;this.updateWorld(world);this.reset(world.x,world.y);}
  get half(){return this.world.size*.46;}
  get foot(){return this.world.size*.07;}
  get height(){return this.world.size*.83;}
  get busy(){return this.motion!=='grounded';}
  get supportId(){return this.support;}
  private enter(m:Motion){this.motion=m;this.age=0;}
  private ledgeAt(x:number,y:number,id?:string){return this.ledges.find(p=>(!id||p.id===id)&&Math.abs(p.y-y)<3&&x>=p.left+this.foot&&x<=p.right-this.foot);}
  private edgeLanding(w:SurfaceWindow,side:number){
    const p=this.ledges.find(p=>p.id===w.id&&(side>0?p.left<=w.x+2:p.right>=w.x+w.width-2));
    return p?clamp((side>0?w.x:w.x+w.width)+side*(this.foot+3),p.left+this.foot,p.right-this.foot):null;
  }
  private attach(p:Ledge|null){this.support=p?.id??null;this.attached=this.world.windows.find(w=>w.id===this.support)??null;}
  reset(x:number,y:number){this.fallOrigin=y;this.deliberateJump=false;this.x=clamp(x,this.half,this.world.width-this.half);this.y=clamp(y,this.height,this.world.height);this.vx=this.vy=0;this.plan=null;this.descent=null;this.corner=null;this.attach(this.ledgeAt(this.x,this.y)??null);this.enter(this.support||this.y>=this.world.height-1?'grounded':'fall');}
  updateWorld(world:SurfaceWorld){
    world={...world,windows:world.windows.map(w=>({...w}))};
    if(world.monitor!==this.world.monitor||world.size!==this.world.size){this.world=world;this.ledges=exposedLedges(world);this.attached=null;this.support=null;this.reset(world.x,world.y);return;}
    this.world=world;this.ledges=exposedLedges(world);
    if(this.attached){
      const next=world.windows.find(w=>w.id===this.attached!.id);
      if(next){
        const old=this.attached;
        if(this.climbing()){
          const dx=(this.side===1?next.x:next.x+next.width)-(this.side===1?old.x:old.x+old.width);
          this.x+=dx;this.y+=next.y-old.y;this.pullStart.x+=dx;this.pullStart.y+=next.y-old.y;
        }else{
          // Follow the same window ID, preserving horizontal relative position on resize.
          this.x=next.x+(this.x-old.x)/old.width*next.width;this.y+=next.y-old.y;
        }
        this.attached=next;
        const climbing=this.climbing();
        if((!climbing&&!this.ledgeAt(this.x,this.y,next.id))||(climbing&&!this.climbVisible(next))||this.x<this.half||this.x>world.width-this.half||this.y<this.height||this.y>world.height)this.fall();
      }else this.fall();
    }
    this.x=clamp(this.x,this.half,world.width-this.half);this.y=clamp(this.y,this.height,world.height);
  }
  private fall(){this.fallOrigin=this.y;this.deliberateJump=false;this.attach(null);this.plan=null;this.descent=null;this.corner=null;this.vx=this.vy=0;this.enter('fall');}
  /** Trapped escape: walk to the nearer end of the ledge part we stand on, then hop off it. */
  private cornerStand(side:1|-1){if(!this.support)return null;const p=this.ledgeAt(this.x,this.y,this.support);return p?(side>0?p.right-this.foot:p.left+this.foot):null;}
  private seekCorner(){let best:{side:1|-1;cost:number}|null=null;for(const side of [1,-1] as const){const s=this.cornerStand(side);if(s!==null){const cost=Math.abs(s-this.x);if(!best||cost<best.cost)best={side,cost};}}this.corner=best?.side??null;}
  private approachCorner(distance:number){
    if(this.corner===null)return false;
    const stand=this.cornerStand(this.corner);if(stand===null){this.corner=null;return false;}
    const dx=stand-this.x;if(Math.abs(dx)>.01)this.direction=Math.sign(dx);
    this.x+=clamp(dx,-distance,distance);
    if(Math.abs(stand-this.x)<.01){const side=this.corner;this.corner=null;this.direction=side;const drop=this.dropPlan();if(drop){this.plan=drop;this.lastChoice='hop-down';this.stuck=0;this.enter('prepare');}else this.cooldown=2;}
    return true;
  }
  private climbing(){return ['grab','climb','pull','lower','descend'].includes(this.motion);}
  private climbVisible(w:SurfaceWindow){return this.wallVisible(w,this.side,Math.min(w.y,this.y-this.height),this.y);}
  /** The strip beside the window edge between top and bottom is not covered by a window in front. */
  private wallVisible(w:SurfaceWindow,side:number,top:number,bottom:number){
    const edge=side===1?w.x:w.x+w.width;
    if(w.y<this.height||edge<this.half*2||edge>this.world.width-this.half*2)return false;
    const left=side===1?edge-this.half*2:edge;
    return !this.world.windows.slice(0,this.world.windows.findIndex(v=>v.id===w.id)).some(v=>v.x<left+this.half*2&&v.x+v.width>left&&v.y<bottom&&v.y+v.height>top);
  }
  /** Restlessness on windows: grows with time away from the floor and with altitude. */
  get descentUrge(){if(!this.support)return 0;const altitude=clamp((this.world.height-this.y)/this.world.height,0,1);return clamp((this.perch-25)/95,0,1)*(.5+.5*altitude);}
  private floorBelow(x:number,y:number):{ledge:Ledge|null;y:number}{const p=this.ledges.filter(p=>p.y>=y&&x>=p.left+this.foot&&x<=p.right-this.foot).sort((a,b)=>a.y-b.y)[0]??null;return {ledge:p,y:p?.y??this.world.height};}
  /** Walk along our own support to its exposed corner, then rope down that side. */
  descentTarget(side:1|-1):{w:SurfaceWindow;stand:number;hang:number}|null{
    const w=this.attached;if(!w||this.support!==w.id)return null;
    const stand=this.edgeLanding(w,side);if(stand===null)return null;
    const here=this.ledgeAt(this.x,this.y,w.id);if(!here||stand<here.left+this.foot||stand>here.right-this.foot)return null;
    const edge=side===1?w.x:w.x+w.width,hang=edge-side*this.half;
    if(hang<this.half||hang>this.world.width-this.half)return null;
    const landing=this.floorBelow(hang,w.y+1).y;
    if(landing-w.y<this.height||!this.wallVisible(w,side,w.y,landing))return null;
    return {w,stand,hang};
  }
  private seekDescent(){let best:{side:1|-1;cost:number}|null=null;for(const side of [1,-1] as const){const t=this.descentTarget(side);if(t){const cost=Math.abs(t.stand-this.x);if(!best||cost<best.cost)best={side,cost};}}this.descent=best?.side??null;}
  private approachDescent(distance:number){
    if(this.descent===null)return false;
    const t=this.descentTarget(this.descent);if(!t){this.descent=null;this.cooldown=2;return false;}
    const dx=t.stand-this.x;if(Math.abs(dx)>.01)this.direction=Math.sign(dx);
    this.x+=clamp(dx,-distance,distance);
    if(Math.abs(t.stand-this.x)<.01){this.side=this.descent;this.direction=this.descent;this.descent=null;this.pullStart={x:this.x,y:this.y};this.enter('lower');}
    return true;
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
  private arc(x:number,y:number,id:string|null){
    const g=900,dx=x-this.x;
    // Jumps across need room to read as a jump; a drop only needs to clear our own ledge end.
    const least=y>this.y+1?this.foot*2:this.half;
    if(Math.abs(dx)<least||Math.abs(dx)>300)return [];
    const rise=Math.max(70,this.y-y+55),apex=this.y-rise;
    if(apex<this.height+5)return [];
    const vy=-Math.sqrt(2*g*rise),time=(-vy+Math.sqrt(vy*vy+2*g*(y-this.y)))/g;
    if(time<=0||Math.abs(dx/time)>350)return [];
    // A drop must have cleared our own ledge when the arc comes back down to our height.
    if(y>this.y+1&&this.support){const own=this.ledgeAt(this.x,this.y,this.support);if(own){const back=this.x+dx/time*(2*-vy/g);if(back>own.left+this.foot-2&&back<own.right-this.foot+2)return [];}}
    return [{x,y,vx:dx/time,vy,id}];
  }
  /** `far` lifts the 220px range for destinations below us: the escape route when every rope and short hop is blocked. */
  private jumpPlans(far=false){
    const own=this.support?this.ledgeAt(this.x,this.y,this.support):null;
    const plans=this.ledges.filter(p=>{const dy=p.y-this.y;return p.id!==this.support&&dy>-220&&(dy<220||far);}).flatMap(p=>{
      const margin=Math.min(8,Math.max(0,(p.right-p.left)/2-this.foot));
      const lo=p.left+this.foot+margin,hi=p.right-this.foot-margin;
      const xs=[clamp(this.x,lo,hi)];
      // A ledge below us is also reached by stepping off either end of our own.
      if(p.y>this.y&&own)for(const edge of [own.right+this.half,own.left-this.half])if(edge>=lo&&edge<=hi)xs.push(edge);
      return xs.flatMap(x=>this.arc(x,p.y,p.id));
    });
    // A short hop from a corner of our window straight down to the floor.
    const p=this.support&&(this.world.height-this.y<220||far)?this.ledgeAt(this.x,this.y,this.support):null;
    if(p)for(const side of [1,-1]){const x=clamp(side>0?p.right+this.half:p.left-this.half,this.half,this.world.width-this.half);if(Math.abs(x-this.x)<=this.half*2.5)plans.push(...this.arc(x,this.world.height,null));}
    return plans;
  }
  private jumpPlan(){const plans=this.jumpPlans();return plans[Math.floor(this.random()*plans.length)]??null;}
  /** Only destinations below us: a hop beats a rope when the drop is short. */
  private dropPlan(){const plans=this.jumpPlans(this.stuck>=SurfaceMotion.STUCK_AFTER).filter(p=>p.y>this.y+1);return plans[Math.floor(this.random()*plans.length)]??null;}
  step(seconds:number,input:MotionInput){
    // Substeps prevent tunnelling through narrow ledges even after delayed frames.
    const dt=clamp(seconds,0,.1),n=Math.max(1,Math.ceil(dt/(1/60)));
    for(let i=0;i<n;i++)this.tick(dt/n,input);
    return {x:this.x,y:this.y,motion:this.motion,direction:this.direction,age:this.age};
  }
  get ropeAnchor(){const w=this.attached;return w&&this.climbing()?{x:this.side===1?w.x:w.x+w.width,y:w.y}:null;}
  private tick(dt:number,input:MotionInput){
    if(input.paused)return;
    const oldAge=this.age;this.age+=dt;this.cooldown=Math.max(0,this.cooldown-dt);
    if(this.support){this.perch+=dt;if(this.descentUrge>=.5&&!['rope-down','hop-down'].includes(this.lastChoice)&&!this.plan&&this.descent===null)this.stuck+=dt;}else if(['grounded','land','hurt'].includes(this.motion)){this.perch=0;this.stuck=0;}
    const here=clamp((this.x-this.world.width/2)/(this.world.width/2),-1,1);this.balance+=(here-this.balance)*Math.min(1,dt/300);
    if(input.reduced){
      this.descent=null;
      // Keep stable surfaces; resolve loss of support without showing a falling animation.
      if(this.motion!=='grounded'){
        const p=this.ledges.filter(p=>p.y>=this.y-3&&this.x>=p.left+this.foot&&this.x<=p.right-this.foot).sort((a,b)=>a.y-b.y)[0];
        this.y=p?.y??this.world.height;this.attach(p??null);this.enter('grounded');this.vx=this.vy=0;
      }
      return;
    }
    if(this.motion==='hurt'){if(this.age>=2.8)this.enter('grounded');return;}
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
      const targetX=this.edgeLanding(w,this.side);if(targetX===null){this.fall();return;}
      const t=smooth(Math.min(1,this.age/.7));
      this.x=this.pullStart.x+(targetX-this.pullStart.x)*t;this.y=this.pullStart.y+(w.y-this.pullStart.y)*t;
      if(t>=1){const p=this.ledgeAt(this.x,this.y,w.id);if(p){this.attach(p);this.enter('grounded');this.climbed=true;this.cooldown=6;}else this.fall();}return;
    }
    if(this.motion==='lower'){
      const w=this.attached;if(!w||this.edgeLanding(w,this.side)===null){this.fall();return;}
      const edge=this.side===1?w.x:w.x+w.width,hang=edge-this.side*this.half,t=smooth(Math.min(1,this.age/.7));
      this.x=this.pullStart.x+(hang-this.pullStart.x)*t;this.y=this.pullStart.y+(w.y+this.height*.6-this.pullStart.y)*t;
      if(t>=1)this.enter('descend');return;
    }
    if(this.motion==='descend'){
      if(!this.attached){this.fall();return;}
      const below=this.floorBelow(this.x,this.y-1);
      this.y=Math.min(below.y,this.y+(climbDistance(this.age,this.world.size)-climbDistance(oldAge,this.world.size)));
      if(this.y>=below.y-.1){this.y=below.y;this.attach(below.ledge);this.vx=this.vy=0;this.enter('grounded');this.climbed=true;this.cooldown=5;}return;
    }
    if(this.motion==='prepare'){
      if(!input.autonomous||!this.plan||(this.plan.id!==null&&!this.ledges.some(p=>p.id===this.plan!.id&&Math.abs(p.y-this.plan!.y)<3&&this.plan!.x>=p.left+this.foot&&this.plan!.x<=p.right-this.foot))){this.plan=null;this.enter('grounded');return;}
      if(this.age>=.28){const p=this.plan;this.attach(null);this.vx=p.vx;this.vy=p.vy;this.direction=Math.sign(this.vx)||1;this.deliberateJump=true;this.enter('jump');}return;
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
      // Exact acceleration keeps the planned landing invariant across frame rates.
      const accelerating=clamp((700-this.vy)/900,0,dt),dy=this.vy*accelerating+450*accelerating*accelerating+700*(dt-accelerating);
      this.vy=Math.min(700,this.vy+900*dt);this.x=clamp(this.x+this.vx*dt,this.half,this.world.width-this.half);this.y=Math.max(this.height,this.y+dy);
      if(this.y<=this.height&&this.vy<0)this.vy=0;
      if(this.vy>=0){
        const p=this.ledges.filter(p=>{
          if(p.y<oldY-2||p.y>this.y)return false;
          const x=oldX+(this.x-oldX)*clamp((p.y-oldY)/(this.y-oldY||1),0,1);
          return x>=p.left+this.foot&&x<=p.right-this.foot;
        }).sort((a,b)=>a.y-b.y)[0];
        if(p||this.y>=this.world.height){this.y=p?.y??this.world.height;if(p)this.x=clamp(this.x,p.left+this.foot,p.right-this.foot);this.attach(p??null);this.vx=this.vy=0;this.plan=null;this.enter(!this.deliberateJump&&this.y-this.fallOrigin>this.height*1.2?'hurt':'land');this.cooldown=5;}
      }return;
    }
    if(this.support&&!this.ledgeAt(this.x,this.y,this.support)){this.fall();return;}
    if(!this.support&&this.y<this.world.height-1){this.fall();return;}
    if(!input.autonomous){this.descent=null;return;}
    if(this.descent!==null&&this.approachDescent(Math.abs(input.walk)*dt))return;
    if(this.corner!==null&&this.approachCorner(Math.abs(input.walk)*dt))return;
    if(this.cooldown===0){
      this.cooldown=8+this.random()*8;
      // Restless on a window: hop down when the drop is short, otherwise rope down our own side.
      if(input.walk>0&&this.random()<this.descentUrge){
        const drop=this.dropPlan();if(drop){this.plan=drop;this.lastChoice='hop-down';this.stuck=0;this.enter('prepare');return;}
        this.seekDescent();if(this.approachDescent(Math.abs(input.walk)*dt)){this.lastChoice='rope-down';this.stuck=0;return;}
        this.lastChoice=!this.descentTarget(1)&&!this.descentTarget(-1)?'no-way-down':'descent-blocked';
        // Trapped long enough: walk to the end of this ledge and hop off it.
        if(this.stuck>=SurfaceMotion.STUCK_AFTER){this.seekCorner();if(this.approachCorner(Math.abs(input.walk)*dt)){this.lastChoice='corner-hop';return;}}
      }
      const plan=this.jumpPlan();if(plan&&this.random()<.55){this.plan=plan;this.lastChoice='jump';this.enter('prepare');return;}
    }
    const dx=input.walk*this.direction*dt;
    if(!dx)return;
    // Bumping into a wall starts a climb only while content up here; a restless character would re-climb the window it just left.
    if(this.descentUrge<.5&&this.startClimb(dx))return;
    const next=this.x+dx,p=this.support?this.ledgeAt(this.x,this.y,this.support):null;
    const left=p?p.left+this.foot:this.half,right=p?p.right-this.foot:this.world.width-this.half;
    if(next<left||next>right){this.x=clamp(next,left,right);this.enter('wobble');return;}
    this.x=next;
  }
}
