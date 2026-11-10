<script lang="ts">
  import { tick } from 'svelte';
  let {value,min,max,step=1,unit='',label,display,disabled=false,allowSilence=false,oncommit,onsilence}:{value:number;min:number;max:number;step?:number;unit?:string;label:string;display?:string;disabled?:boolean;allowSilence?:boolean;oncommit:(value:number)=>void;onsilence?:()=>void}=$props();
  let editing=$state(false),draft=$state(''),error=$state('');
  let input=$state<HTMLInputElement>();
  let button=$state<HTMLButtonElement>();
  const id=$props.id();
  const decimals=$derived(unit==='%'?0:2);
  function rounded(n:number){return Number(n.toFixed(decimals));}
  const readout=$derived(display===undefined?`${rounded(value)} ${unit}`:display.replace(/[+-]?\d+(?:\.\d+)?/g,n=>String(rounded(Number(n)))));
  const range=$derived(`${allowSilence?'−∞ or ':''}${rounded(min)} to ${rounded(max)} ${unit}`.trim());
  async function begin(){if(disabled)return;draft=Number.isFinite(value)?String(rounded(value)):'';error='';editing=true;await tick();input?.focus();input?.select();}
  function parse(){if(allowSilence&&['-inf','-infinity','−∞','-∞'].includes(draft.trim().toLowerCase()))return {silence:true,value:0};const n=Number(draft);if(!draft.trim()||!Number.isFinite(n)||n<min||n>max)return null;return {silence:false,value:Math.min(max,Math.max(min,rounded(n)))};}
  function commit(refocus=false){if(!editing)return true;const next=parse();if(!next){error=`Enter ${range}.`;return false;}editing=false;error='';if(next.silence)onsilence?.();else oncommit(next.value);if(refocus)tick().then(()=>button?.focus());return true;}
  function cancel(refocus=false){editing=false;error='';if(refocus)tick().then(()=>button?.focus());}
  function hintPortal(node:HTMLElement){document.body.append(node);const position=()=>{if(!input)return;const r=input.getBoundingClientRect();const width=Math.min(240,innerWidth-16);node.style.width=width+'px';node.style.left=Math.max(8,Math.min(r.left,innerWidth-width-8))+'px';node.style.top=(r.bottom+80<innerHeight?r.bottom+8:Math.max(8,r.top-node.offsetHeight-8))+'px';};position();document.addEventListener('scroll',position,true);window.addEventListener('resize',position);return{destroy(){document.removeEventListener('scroll',position,true);window.removeEventListener('resize',position);node.remove();}};}
</script>

<span class="precise-value" class:editing>
 {#if editing}<span class="precise-edit-field"><input bind:this={input} class="precise-input" type="text" inputmode="decimal" bind:value={draft} aria-label={`Edit ${label}`} aria-describedby={`${id}-hint`} aria-invalid={!!error} role="spinbutton" aria-valuemin={min} aria-valuemax={max} aria-valuenow={Number.isFinite(Number(draft))?Number(draft):undefined} oninput={()=>error=''} onkeydown={e=>{e.stopPropagation();if(e.key==='Enter'){e.preventDefault();commit(true);}else if(e.key==='Escape'){e.preventDefault();cancel(true);}else if(e.key==='ArrowUp'||e.key==='ArrowDown'){e.preventDefault();const n=Number(draft);draft=String(Number(Math.max(min,Math.min(max,(Number.isFinite(n)?n:value)+(e.key==='ArrowUp'?Math.max(step,10**-decimals):-Math.max(step,10**-decimals)))).toFixed(decimals)));}}} onblur={()=>{if(!commit())cancel();}}/><span use:hintPortal id={`${id}-hint`} class="precise-hint" class:invalid={!!error} role={error?'alert':undefined}>{error||`${range} · Enter to apply · Esc to cancel`}</span></span>
 {:else}<button bind:this={button} type="button" class="precise-trigger" {disabled} aria-label={`${label}: ${readout}. Double-click or press Enter to edit.`} data-tooltip={`Double-click or press Enter to edit ${label}. Range: ${range}.`} ondblclick={e=>{e.preventDefault();e.stopPropagation();begin();}} onclick={e=>e.preventDefault()} onkeydown={e=>{if(e.key==='Enter'||e.key==='F2'){e.preventDefault();e.stopPropagation();begin();}}}>{readout}</button>{/if}
</span>


