/** Shared settings/preview editor. Each offset is unique; at least one stays enabled. */
export class ReminderEditor {
 private values:number[]=[5];
 constructor(private doc:Document,private change:(values:number[])=>void){
  doc.getElementById('calendar-reminder-form')!.onsubmit=e=>{
   e.preventDefault();const input=doc.getElementById('calendar-reminder-minutes') as HTMLInputElement;
   if(!input.reportValidity())return;const minutes=Number(input.value);
   if(this.values.includes(minutes)){this.message('이미 추가된 알림 시점이에요.');return;}
   if(this.values.length>=8){this.message('알림은 최대 8개까지 추가할 수 있어요.');return;}
   this.commit([...this.values,minutes]);
  };
 }
 private message(text:string){this.doc.getElementById('calendar-reminder-help')!.textContent=text;}
 private commit(values:number[]){this.render(values);this.change([...this.values]);this.message('알림 시점을 저장했어요.');}
 render(values:number[]){
  this.values=[...new Set(values)].sort((a,b)=>b-a);
  const root=this.doc.getElementById('calendar-reminders')!;root.replaceChildren();
  for(const n of this.values){
   const chip=this.doc.createElement('span');chip.className='reminder-chip';
   const label=n===0?'시작 시각':`${n}분 전`;chip.append(label);
   const remove=this.doc.createElement('button');remove.type='button';remove.textContent='×';
   remove.setAttribute('aria-label',`${label} 알림 삭제`);remove.disabled=this.values.length===1;
   remove.onclick=()=>this.commit(this.values.filter(v=>v!==n));chip.append(remove);root.append(chip);
  }
  (this.doc.getElementById('calendar-reminder-add') as HTMLButtonElement).disabled=this.values.length>=8;
 }
}
