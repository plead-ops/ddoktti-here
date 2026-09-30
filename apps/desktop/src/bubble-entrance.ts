/** IDs are marked only when the bubble actually becomes visible (after landing, etc.). */
export class BubbleEntrance {
 private seen=new Set<string>();
 show(el:HTMLElement,id:string|undefined,reduced:boolean){
  if(el.hidden||!id)return;
  if(reduced||matchMedia('(prefers-reduced-motion: reduce)').matches){el.classList.remove('bubble-enter');this.seen.add(id);return;}
  if(this.seen.has(id))return;
  this.seen.add(id);if(this.seen.size>256)this.seen.delete(this.seen.values().next().value!);
  el.classList.remove('bubble-enter');void el.offsetWidth;el.classList.add('bubble-enter');
  el.addEventListener('animationend',e=>{if(e.animationName==='bubble-arrive')el.classList.remove('bubble-enter');},{once:true});
 }
}
