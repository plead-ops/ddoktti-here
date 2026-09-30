import gaits from './pet-gaits.json';
/** Stride is the distance of a complete cycle in the 400×260 artwork coordinate system. */
export function gaitSpeed(mode:string,size:number,speed=1){
 const g=gaits[mode as keyof typeof gaits];return g?g.stride*(size/260)/(g.frames*g.frameMs/1000)*speed:0;
}
/** Advance the drawing from distance actually travelled, excluding moving-window displacement. */
export function gaitElapsed(mode:string,distance:number,size:number){const speed=gaitSpeed(mode,size);return speed>0?Math.max(0,distance)/speed*1000:0;}
export function gaitFrame(mode:'walk'|'run',elapsed:number,reduced=false){const g=gaits[mode];return reduced?mode==='run'?3:0:Math.floor(Math.max(0,elapsed)/g.frameMs)%g.frames;}
