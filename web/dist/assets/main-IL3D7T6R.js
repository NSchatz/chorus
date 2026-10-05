function Y(s){let e=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(e/1e3)}.${String(e%1e3).padStart(3,"0")}`}function Kt(s,e){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${Y(e)}}`}function Yt(s,e){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${e?"true":"false"}}`}function Ue(s,e){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(e)}}`}function ie(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function Xt(s,e){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(e)}}`}function Qt(s,e){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${Y(e)}}`}var K=Object.freeze({min:-10,max:10}),qs=["bass","treble"],Vs=["loudness","night","speech"];function ne(s,e={}){let t=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of qs){if(e[r]===void 0)continue;let a=Math.min(K.max,Math.max(K.min,Math.round(Number(e[r])||0)));t+=`,"${r}":${a}`}for(let r of Vs)e[r]!==void 0&&(t+=`,"${r}":${e[r]?"true":"false"}`);return e.tv_upmix!==void 0&&(t+=`,"tv_upmix":${JSON.stringify(String(e.tv_upmix))}`),`${t}}`}var Zt=Object.freeze(["off","ambient"]),X=Object.freeze({min:-100,max:200}),Fe=(s,{min:e,max:t})=>Math.min(t,Math.max(e,Math.round(Number(s)||0)));function De(s,e){return`{"v":2,"t":"av_trim","zone":${JSON.stringify(s)},"av_trim_ms":${Fe(e,X)}}`}var je=Object.freeze({min:40,max:200}),Be=Object.freeze({min:-1200,max:600}),er=Object.freeze(["normal","inverted"]);function He(s){let e=Fe(s,Be),t=Math.abs(e);return`${e<0?"-":""}${Math.floor(t/100)}.${String(t%100).padStart(2,"0")}`}function le(s,e={}){let t=`{"v":2,"t":"bass_management","zone":${JSON.stringify(s)}`;return e.crossover_hz!==void 0&&(t+=`,"crossover_hz":${Fe(e.crossover_hz,je)}`),e.sub_level_db!==void 0&&(t+=`,"sub_level_db":${He(e.sub_level_db)}`),e.sub_polarity!==void 0&&(t+=`,"sub_polarity":${JSON.stringify(String(e.sub_polarity))}`),`${t}}`}function tr(s,e){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${Y(e)}}`}var O=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),We=8;function rr(s,e=[]){let t=e.map(r=>{let a=O.filter(o=>(r.days??[]).includes(o));return`{"days":${JSON.stringify(a)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${Y(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${t.join(",")}]}`}function sr(s,e){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${e?"true":"false"}}`}function ar(s,e,t,{stopOnStandby:r=!0,lowLatency:a=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(e)},"enabled":${t?"true":"false"}${r===!1?',"stop_on_standby":false':""}${a===!1?',"low_latency":false':""}}`}var de=600,ue=720,ce=720,Pe=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function qe({alarm:s,target:e,time:t,days:r=[],source:a,volume:o,rampS:i,durationMin:u,enabled:l}){let p=O.filter(m=>r.includes(m));return`{"v":2,"t":"alarm_set","alarm":${JSON.stringify(s)},"target":${JSON.stringify(e)},"time":${JSON.stringify(String(t))},"days":${JSON.stringify(p)},"source":${JSON.stringify(a)},"volume":${Y(o)},"ramp_s":${Pe(i,de)},"duration_min":${Pe(u,ue)},"enabled":${l?"true":"false"}}`}function or(s){return`{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(s)}}`}function ir(s){return`{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(s)}}`}function Ve(s,e){return`{"v":2,"t":"sleep","target":${JSON.stringify(s)},"minutes":${Pe(e,ce)}}`}function nr(s,e,t,r){return`{"v":2,"t":"source_store","id":${JSON.stringify(s)},"kind":${JSON.stringify(e)},"value":${JSON.stringify(t)},"name":${JSON.stringify(r)}}`}function lr(s){return`{"v":2,"t":"source_forget","id":${JSON.stringify(s)}}`}function dr(s,e){return`{"v":2,"t":"speaker_name","speaker":${JSON.stringify(s)},"name":${JSON.stringify(e)}}`}function ur(s,e){return`{"v":2,"t":"speaker_room","speaker":${JSON.stringify(s)},"room":${typeof e=="string"&&e?JSON.stringify(e):"null"}}`}function cr(s){return`{"v":2,"t":"speaker_forget","speaker":${JSON.stringify(s)}}`}function hr(s,e){return`{"v":2,"t":"firmware_install","speaker":${JSON.stringify(s)},"image":${JSON.stringify(e)}}`}function pr(s){return`{"v":2,"t":"firmware_cancel","speaker":${JSON.stringify(s)}}`}function fr(){return'{"v":2,"t":"firmware_rescan"}'}function Js(s){let e=t=>Number(t)||0;return`{"freq_hz":${Math.round(e(s.freq_hz))},"gain_db":${e(s.gain_db).toFixed(2)},"q":${e(s.q).toFixed(3)}}`}function mr(s,e){return`{"v":2,"t":"room_eq","zone":${JSON.stringify(s)},"filters":[${e.map(Js).join(",")}],"enabled":true}`}function he(s,e){return`{"v":2,"t":"room_eq","zone":${JSON.stringify(s)},"enabled":${e?"true":"false"}}`}function gr(s){return`{"v":2,"t":"room_eq_undo","zone":${JSON.stringify(s)}}`}function vr(s){return`{"v":2,"t":"measure_sweep","zone":${JSON.stringify(s)}}`}var Jt=2097152;function Gs(s){return/^([a-z][a-z_-]*): /.exec(String(s??""))?.[1]??""}function Je(s,e,t=""){let r=5381;for(let a of String(t))r=(Math.imul(r,33)^a.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(e)}${t?`#${r.toString(36)}`:""}`}function oe(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var Le="Signed out";async function Gt(s){let e="";try{e=(await s.text()).trim()}catch{e=""}try{let t=JSON.parse(e);if(t&&typeof t.detail=="string"&&t.detail){let r=typeof t.field=="string"&&t.field?{field:t.field}:{};return{refusal:t.detail,...r}}}catch{}return{refusal:e||`the server answered ${s.status}`}}var Ks={set:(s,e)=>globalThis.setTimeout(s,e),clear:s=>globalThis.clearTimeout(s)};function br({fetch:s=globalThis.fetch.bind(globalThis),base:e="../",timers:t=Ks}={}){async function r(){let l=await s(`${e}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(oe(l))throw Object.assign(new Error(Le),{signedOut:!0});if(!l.ok)throw new Error(`the server answered ${l.status}`);return l.json()}async function a(l){let p;try{p=await s(`${e}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:l,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(oe(p))return{ok:!1,refusal:Le,signedOut:!0};if(!p.ok)return{ok:!1,...await Gt(p)};try{return{ok:!0,state:await p.json()}}catch{return{ok:!0,state:null}}}function o({onState:l,onStatus:p=()=>{}}){let m=!1,d=null,f=null,h=null,g=()=>{h!==null&&t.clear(h),h=null},v=()=>{g(),h=t.set(()=>d?.abort(),4e4)},k=w=>{let x=w.split(`
`).filter(J=>J.startsWith("data:")).map(J=>J.slice(5).replace(/^ /,"")).join(`
`);if(!x)return;let M;try{M=JSON.parse(x)}catch{return}p("live"),l(M)};async function _(){d=new AbortController,v();let w=!1;try{let x=await s(`${e}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:d.signal});if(w=oe(x),!x.ok||!x.body)throw new Error(`the server answered ${x.status}`);let M=x.body.getReader();d.signal.addEventListener("abort",()=>M.cancel().catch(()=>{}));let J=new TextDecoder,G="";for(;;){let{done:Hs,value:Ws}=await M.read();if(Hs||m||d.signal.aborted)break;v(),G+=J.decode(Ws,{stream:!0}).replace(/\r\n?/g,`
`);let Ie;for(;(Ie=G.indexOf(`

`))!==-1;)k(G.slice(0,Ie)),G=G.slice(Ie+2)}}catch{}g(),!m&&(p(w?"signed-out":"lost"),f=t.set(()=>{f=null,_()},1e3))}return _(),()=>{m=!0,g(),f!==null&&t.clear(f),d?.abort()}}async function i(l,p,m={}){if(p.byteLength>Jt)return{ok:!1,refusal:`the recording is ${p.byteLength} bytes, over the ${Jt} the server takes`};let d=`zone=${encodeURIComponent(l)}`;Number.isFinite(m.sweepMs)&&(d+=`&sweep_ms=${Math.round(m.sweepMs)}`),Number.isFinite(m.fadeInMs)&&(d+=`&fade_in_ms=${Math.round(m.fadeInMs)}`);let f;try{f=await s(`${e}api/room-fit?${d}`,{method:"POST",headers:{"Content-Type":"audio/wav"},body:p,cache:"no-store",redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(oe(f))return{ok:!1,refusal:Le,signedOut:!0};if(!f.ok){let h=await Gt(f),g=Gs(h.refusal);return{ok:!1,...h,...g?{name:g}:{}}}try{let h=await f.json();if(!h||h.t!=="room_fit"||!Array.isArray(h.filters))throw new Error("not a fit");let g=v=>typeof v=="number"&&Number.isFinite(v)?v:null;return{ok:!0,fit:{filters:h.filters.filter(v=>v&&typeof v=="object").map(({freq_hz:v,gain_db:k,q:_})=>({freq_hz:v,gain_db:k,q:_})),rmsBeforeDb:g(h.rms_before_db),rmsAfterDb:g(h.rms_after_db)}}}catch{return{ok:!1,refusal:"the server's answer was not a fit this page can read"}}}return{state:r,command:a,events:o,artwork:(l,p)=>Je(e,l,p),roomFit:i}}var pe=globalThis,fe=pe.ShadowRoot&&(pe.ShadyCSS===void 0||pe.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,Ge=Symbol(),_r=new WeakMap,Q=class{constructor(e,t,r){if(this._$cssResult$=!0,r!==Ge)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=e,this.t=t}get styleSheet(){let e=this.o,t=this.t;if(fe&&e===void 0){let r=t!==void 0&&t.length===1;r&&(e=_r.get(t)),e===void 0&&((this.o=e=new CSSStyleSheet).replaceSync(this.cssText),r&&_r.set(t,e))}return e}toString(){return this.cssText}},$r=s=>new Q(typeof s=="string"?s:s+"",void 0,Ge),$=(s,...e)=>{let t=s.length===1?s[0]:e.reduce((r,a,o)=>r+(i=>{if(i._$cssResult$===!0)return i.cssText;if(typeof i=="number")return i;throw Error("Value passed to 'css' function must be a 'css' function result: "+i+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(a)+s[o+1],s[0]);return new Q(t,s,Ge)},yr=(s,e)=>{if(fe)s.adoptedStyleSheets=e.map(t=>t instanceof CSSStyleSheet?t:t.styleSheet);else for(let t of e){let r=document.createElement("style"),a=pe.litNonce;a!==void 0&&r.setAttribute("nonce",a),r.textContent=t.cssText,s.appendChild(r)}},Ke=fe?s=>s:s=>s instanceof CSSStyleSheet?(e=>{let t="";for(let r of e.cssRules)t+=r.cssText;return $r(t)})(s):s;var{is:Ys,defineProperty:Xs,getOwnPropertyDescriptor:Qs,getOwnPropertyNames:Zs,getOwnPropertySymbols:ea,getPrototypeOf:ta}=Object,me=globalThis,wr=me.trustedTypes,ra=wr?wr.emptyScript:"",sa=me.reactiveElementPolyfillSupport,Z=(s,e)=>s,Ye={toAttribute(s,e){switch(e){case Boolean:s=s?ra:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,e){let t=s;switch(e){case Boolean:t=s!==null;break;case Number:t=s===null?null:Number(s);break;case Object:case Array:try{t=JSON.parse(s)}catch{t=null}}return t}},Sr=(s,e)=>!Ys(s,e),kr={attribute:!0,type:String,converter:Ye,reflect:!1,useDefault:!1,hasChanged:Sr};Symbol.metadata??=Symbol("metadata"),me.litPropertyMetadata??=new WeakMap;var C=class extends HTMLElement{static addInitializer(e){this._$Ei(),(this.l??=[]).push(e)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(e,t=kr){if(t.state&&(t.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(e)&&((t=Object.create(t)).wrapped=!0),this.elementProperties.set(e,t),!t.noAccessor){let r=Symbol(),a=this.getPropertyDescriptor(e,r,t);a!==void 0&&Xs(this.prototype,e,a)}}static getPropertyDescriptor(e,t,r){let{get:a,set:o}=Qs(this.prototype,e)??{get(){return this[t]},set(i){this[t]=i}};return{get:a,set(i){let u=a?.call(this);o?.call(this,i),this.requestUpdate(e,u,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(e){return this.elementProperties.get(e)??kr}static _$Ei(){if(this.hasOwnProperty(Z("elementProperties")))return;let e=ta(this);e.finalize(),e.l!==void 0&&(this.l=[...e.l]),this.elementProperties=new Map(e.elementProperties)}static finalize(){if(this.hasOwnProperty(Z("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(Z("properties"))){let t=this.properties,r=[...Zs(t),...ea(t)];for(let a of r)this.createProperty(a,t[a])}let e=this[Symbol.metadata];if(e!==null){let t=litPropertyMetadata.get(e);if(t!==void 0)for(let[r,a]of t)this.elementProperties.set(r,a)}this._$Eh=new Map;for(let[t,r]of this.elementProperties){let a=this._$Eu(t,r);a!==void 0&&this._$Eh.set(a,t)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(e){let t=[];if(Array.isArray(e)){let r=new Set(e.flat(1/0).reverse());for(let a of r)t.unshift(Ke(a))}else e!==void 0&&t.push(Ke(e));return t}static _$Eu(e,t){let r=t.attribute;return r===!1?void 0:typeof r=="string"?r:typeof e=="string"?e.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(e=>this.enableUpdating=e),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(e=>e(this))}addController(e){(this._$EO??=new Set).add(e),this.renderRoot!==void 0&&this.isConnected&&e.hostConnected?.()}removeController(e){this._$EO?.delete(e)}_$E_(){let e=new Map,t=this.constructor.elementProperties;for(let r of t.keys())this.hasOwnProperty(r)&&(e.set(r,this[r]),delete this[r]);e.size>0&&(this._$Ep=e)}createRenderRoot(){let e=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return yr(e,this.constructor.elementStyles),e}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(e=>e.hostConnected?.())}enableUpdating(e){}disconnectedCallback(){this._$EO?.forEach(e=>e.hostDisconnected?.())}attributeChangedCallback(e,t,r){this._$AK(e,r)}_$ET(e,t){let r=this.constructor.elementProperties.get(e),a=this.constructor._$Eu(e,r);if(a!==void 0&&r.reflect===!0){let o=(r.converter?.toAttribute!==void 0?r.converter:Ye).toAttribute(t,r.type);this._$Em=e,o==null?this.removeAttribute(a):this.setAttribute(a,o),this._$Em=null}}_$AK(e,t){let r=this.constructor,a=r._$Eh.get(e);if(a!==void 0&&this._$Em!==a){let o=r.getPropertyOptions(a),i=typeof o.converter=="function"?{fromAttribute:o.converter}:o.converter?.fromAttribute!==void 0?o.converter:Ye;this._$Em=a;let u=i.fromAttribute(t,o.type);this[a]=u??this._$Ej?.get(a)??u,this._$Em=null}}requestUpdate(e,t,r,a=!1,o){if(e!==void 0){let i=this.constructor;if(a===!1&&(o=this[e]),r??=i.getPropertyOptions(e),!((r.hasChanged??Sr)(o,t)||r.useDefault&&r.reflect&&o===this._$Ej?.get(e)&&!this.hasAttribute(i._$Eu(e,r))))return;this.C(e,t,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(e,t,{useDefault:r,reflect:a,wrapped:o},i){r&&!(this._$Ej??=new Map).has(e)&&(this._$Ej.set(e,i??t??this[e]),o!==!0||i!==void 0)||(this._$AL.has(e)||(this.hasUpdated||r||(t=void 0),this._$AL.set(e,t)),a===!0&&this._$Em!==e&&(this._$Eq??=new Set).add(e))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(t){Promise.reject(t)}let e=this.scheduleUpdate();return e!=null&&await e,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[a,o]of this._$Ep)this[a]=o;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[a,o]of r){let{wrapped:i}=o,u=this[a];i!==!0||this._$AL.has(a)||u===void 0||this.C(a,void 0,o,u)}}let e=!1,t=this._$AL;try{e=this.shouldUpdate(t),e?(this.willUpdate(t),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(t)):this._$EM()}catch(r){throw e=!1,this._$EM(),r}e&&this._$AE(t)}willUpdate(e){}_$AE(e){this._$EO?.forEach(t=>t.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(e)),this.updated(e)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(e){return!0}update(e){this._$Eq&&=this._$Eq.forEach(t=>this._$ET(t,this[t])),this._$EM()}updated(e){}firstUpdated(e){}};C.elementStyles=[],C.shadowRootOptions={mode:"open"},C[Z("elementProperties")]=new Map,C[Z("finalized")]=new Map,sa?.({ReactiveElement:C}),(me.reactiveElementVersions??=[]).push("2.1.2");var Qe=globalThis,xr=s=>s,ge=Qe.trustedTypes,Ar=ge?ge.createPolicy("lit-html",{createHTML:s=>s}):void 0,Ze="$lit$",R=`lit$${Math.random().toFixed(9).slice(2)}$`,et="?"+R,aa=`<${et}>`,P=document,te=()=>P.createComment(""),re=s=>s===null||typeof s!="object"&&typeof s!="function",tt=Array.isArray,Nr=s=>tt(s)||typeof s?.[Symbol.iterator]=="function",Xe=`[ 	
\f\r]`,ee=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Er=/-->/g,Tr=/>/g,I=RegExp(`>|${Xe}(?:([^\\s"'>=/]+)(${Xe}*=${Xe}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Or=/'/g,Cr=/"/g,Mr=/^(?:script|style|textarea|title)$/i,rt=s=>(e,...t)=>({_$litType$:s,strings:e,values:t}),n=rt(1),Eo=rt(2),To=rt(3),N=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),Rr=new WeakMap,L=P.createTreeWalker(P,129);function zr(s,e){if(!tt(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return Ar!==void 0?Ar.createHTML(e):e}var Ir=(s,e)=>{let t=s.length-1,r=[],a,o=e===2?"<svg>":e===3?"<math>":"",i=ee;for(let u=0;u<t;u++){let l=s[u],p,m,d=-1,f=0;for(;f<l.length&&(i.lastIndex=f,m=i.exec(l),m!==null);)f=i.lastIndex,i===ee?m[1]==="!--"?i=Er:m[1]!==void 0?i=Tr:m[2]!==void 0?(Mr.test(m[2])&&(a=RegExp("</"+m[2],"g")),i=I):m[3]!==void 0&&(i=I):i===I?m[0]===">"?(i=a??ee,d=-1):m[1]===void 0?d=-2:(d=i.lastIndex-m[2].length,p=m[1],i=m[3]===void 0?I:m[3]==='"'?Cr:Or):i===Cr||i===Or?i=I:i===Er||i===Tr?i=ee:(i=I,a=void 0);let h=i===I&&s[u+1].startsWith("/>")?" ":"";o+=i===ee?l+aa:d>=0?(r.push(p),l.slice(0,d)+Ze+l.slice(d)+R+h):l+R+(d===-2?u:h)}return[zr(s,o+(s[t]||"<?>")+(e===2?"</svg>":e===3?"</math>":"")),r]},se=class s{constructor({strings:e,_$litType$:t},r){let a;this.parts=[];let o=0,i=0,u=e.length-1,l=this.parts,[p,m]=Ir(e,t);if(this.el=s.createElement(p,r),L.currentNode=this.el.content,t===2||t===3){let d=this.el.content.firstChild;d.replaceWith(...d.childNodes)}for(;(a=L.nextNode())!==null&&l.length<u;){if(a.nodeType===1){if(a.hasAttributes())for(let d of a.getAttributeNames())if(d.endsWith(Ze)){let f=m[i++],h=a.getAttribute(d).split(R),g=/([.?@])?(.*)/.exec(f);l.push({type:1,index:o,name:g[2],strings:h,ctor:g[1]==="."?be:g[1]==="?"?_e:g[1]==="@"?$e:F}),a.removeAttribute(d)}else d.startsWith(R)&&(l.push({type:6,index:o}),a.removeAttribute(d));if(Mr.test(a.tagName)){let d=a.textContent.split(R),f=d.length-1;if(f>0){a.textContent=ge?ge.emptyScript:"";for(let h=0;h<f;h++)a.append(d[h],te()),L.nextNode(),l.push({type:2,index:++o});a.append(d[f],te())}}}else if(a.nodeType===8)if(a.data===et)l.push({type:2,index:o});else{let d=-1;for(;(d=a.data.indexOf(R,d+1))!==-1;)l.push({type:7,index:o}),d+=R.length-1}o++}}static createElement(e,t){let r=P.createElement("template");return r.innerHTML=e,r}};function U(s,e,t=s,r){if(e===N)return e;let a=r!==void 0?t._$Co?.[r]:t._$Cl,o=re(e)?void 0:e._$litDirective$;return a?.constructor!==o&&(a?._$AO?.(!1),o===void 0?a=void 0:(a=new o(s),a._$AT(s,t,r)),r!==void 0?(t._$Co??=[])[r]=a:t._$Cl=a),a!==void 0&&(e=U(s,a._$AS(s,e.values),a,r)),e}var ve=class{constructor(e,t){this._$AV=[],this._$AN=void 0,this._$AD=e,this._$AM=t}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(e){let{el:{content:t},parts:r}=this._$AD,a=(e?.creationScope??P).importNode(t,!0);L.currentNode=a;let o=L.nextNode(),i=0,u=0,l=r[0];for(;l!==void 0;){if(i===l.index){let p;l.type===2?p=new H(o,o.nextSibling,this,e):l.type===1?p=new l.ctor(o,l.name,l.strings,this,e):l.type===6&&(p=new ye(o,this,e)),this._$AV.push(p),l=r[++u]}i!==l?.index&&(o=L.nextNode(),i++)}return L.currentNode=P,a}p(e){let t=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(e,r,t),t+=r.strings.length-2):r._$AI(e[t])),t++}},H=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(e,t,r,a){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=e,this._$AB=t,this._$AM=r,this.options=a,this._$Cv=a?.isConnected??!0}get parentNode(){let e=this._$AA.parentNode,t=this._$AM;return t!==void 0&&e?.nodeType===11&&(e=t.parentNode),e}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(e,t=this){e=U(this,e,t),re(e)?e===c||e==null||e===""?(this._$AH!==c&&this._$AR(),this._$AH=c):e!==this._$AH&&e!==N&&this._(e):e._$litType$!==void 0?this.$(e):e.nodeType!==void 0?this.T(e):Nr(e)?this.k(e):this._(e)}O(e){return this._$AA.parentNode.insertBefore(e,this._$AB)}T(e){this._$AH!==e&&(this._$AR(),this._$AH=this.O(e))}_(e){this._$AH!==c&&re(this._$AH)?this._$AA.nextSibling.data=e:this.T(P.createTextNode(e)),this._$AH=e}$(e){let{values:t,_$litType$:r}=e,a=typeof r=="number"?this._$AC(e):(r.el===void 0&&(r.el=se.createElement(zr(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===a)this._$AH.p(t);else{let o=new ve(a,this),i=o.u(this.options);o.p(t),this.T(i),this._$AH=o}}_$AC(e){let t=Rr.get(e.strings);return t===void 0&&Rr.set(e.strings,t=new se(e)),t}k(e){tt(this._$AH)||(this._$AH=[],this._$AR());let t=this._$AH,r,a=0;for(let o of e)a===t.length?t.push(r=new s(this.O(te()),this.O(te()),this,this.options)):r=t[a],r._$AI(o),a++;a<t.length&&(this._$AR(r&&r._$AB.nextSibling,a),t.length=a)}_$AR(e=this._$AA.nextSibling,t){for(this._$AP?.(!1,!0,t);e!==this._$AB;){let r=xr(e).nextSibling;xr(e).remove(),e=r}}setConnected(e){this._$AM===void 0&&(this._$Cv=e,this._$AP?.(e))}},F=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(e,t,r,a,o){this.type=1,this._$AH=c,this._$AN=void 0,this.element=e,this.name=t,this._$AM=a,this.options=o,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(e,t=this,r,a){let o=this.strings,i=!1;if(o===void 0)e=U(this,e,t,0),i=!re(e)||e!==this._$AH&&e!==N,i&&(this._$AH=e);else{let u=e,l,p;for(e=o[0],l=0;l<o.length-1;l++)p=U(this,u[r+l],t,l),p===N&&(p=this._$AH[l]),i||=!re(p)||p!==this._$AH[l],p===c?e=c:e!==c&&(e+=(p??"")+o[l+1]),this._$AH[l]=p}i&&!a&&this.j(e)}j(e){e===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,e??"")}},be=class extends F{constructor(){super(...arguments),this.type=3}j(e){this.element[this.name]=e===c?void 0:e}},_e=class extends F{constructor(){super(...arguments),this.type=4}j(e){this.element.toggleAttribute(this.name,!!e&&e!==c)}},$e=class extends F{constructor(e,t,r,a,o){super(e,t,r,a,o),this.type=5}_$AI(e,t=this){if((e=U(this,e,t,0)??c)===N)return;let r=this._$AH,a=e===c&&r!==c||e.capture!==r.capture||e.once!==r.once||e.passive!==r.passive,o=e!==c&&(r===c||a);a&&this.element.removeEventListener(this.name,this,r),o&&this.element.addEventListener(this.name,this,e),this._$AH=e}handleEvent(e){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,e):this._$AH.handleEvent(e)}},ye=class{constructor(e,t,r){this.element=e,this.type=6,this._$AN=void 0,this._$AM=t,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(e){U(this,e)}},Lr={M:Ze,P:R,A:et,C:1,L:Ir,R:ve,D:Nr,V:U,I:H,H:F,N:_e,U:$e,B:be,F:ye},oa=Qe.litHtmlPolyfillSupport;oa?.(se,H),(Qe.litHtmlVersions??=[]).push("3.3.3");var Pr=(s,e,t)=>{let r=t?.renderBefore??e,a=r._$litPart$;if(a===void 0){let o=t?.renderBefore??null;r._$litPart$=a=new H(e.insertBefore(te(),o),o,void 0,t??{})}return a._$AI(s),a};var st=globalThis,b=class extends C{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let e=super.createRenderRoot();return this.renderOptions.renderBefore??=e.firstChild,e}update(e){let t=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(e),this._$Do=Pr(t,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return N}};b._$litElement$=!0,b.finalized=!0,st.litElementHydrateSupport?.({LitElement:b});var ia=st.litElementPolyfillSupport;ia?.({LitElement:b});(st.litElementVersions??=[]).push("4.2.2");function na(s,e,t){let r=s.elementFromPoint?.(e,t)??null;for(;r?.shadowRoot?.elementFromPoint;){let a=r.shadowRoot.elementFromPoint(e,t);if(!a||a===r)break;r=a}return r}function la(s){for(let e=s;e;e=e.assignedSlot??e.parentNode??e.host){let t=e.dataset?.drop;if(t==="alone")return{kind:t};if((t==="room"||t==="group")&&e.dataset.dropId)return{kind:t,id:e.dataset.dropId}}return null}var Ur=(s,e,t)=>la(na(s,e,t));function Fr({root:s=document,onStart:e=()=>{},onOver:t=()=>{},onEnd:r=()=>{}}={}){let a=null,o=()=>{let{handle:d,pointerId:f}=a;d.removeEventListener("pointermove",i),d.removeEventListener("pointerup",u),d.removeEventListener("pointercancel",l),d.removeEventListener("lostpointercapture",l),s.removeEventListener("keydown",p,!0);try{d.releasePointerCapture?.(f)}catch{}a=null};function i(d){if(!(!a||d.pointerId!==a.pointerId)){if(!a.moving){if(Math.hypot(d.clientX-a.x,d.clientY-a.y)<8)return;a.moving=!0,e(a.room)}d.preventDefault(),t(Ur(s,d.clientX,d.clientY))}}function u(d){if(!a||d.pointerId!==a.pointerId)return;let{room:f,moving:h}=a;if(o(),!h)return;let g=v=>{v.stopPropagation(),v.preventDefault()};s.addEventListener("click",g,!0),setTimeout(()=>s.removeEventListener("click",g,!0),0),r(f,Ur(s,d.clientX,d.clientY))}function l(d){if(!a||d&&d.pointerId!==void 0&&d.pointerId!==a.pointerId)return;let{room:f,moving:h}=a;o(),h&&r(f,null)}function p(d){d.key==="Escape"&&l()}function m(d){if(a||d.isPrimary===!1||d.button>0)return;let f=d.composedPath().find(h=>h.dataset?.dragRoom);if(f){a={handle:f,room:f.dataset.dragRoom,pointerId:d.pointerId,x:d.clientX,y:d.clientY,moving:!1};try{f.setPointerCapture?.(d.pointerId)}catch{}f.addEventListener("pointermove",i),f.addEventListener("pointerup",u),f.addEventListener("pointercancel",l),f.addEventListener("lostpointercapture",l),s.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!a?.moving}}var we=[],jr=s=>String(s).split("/").filter(Boolean);function S(s){let{id:e,path:t,title:r,render:a}=s??{};if(typeof e!="string"||!e||e==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof a!="function")throw new Error(`the screen '${e}' has a title and a render`);let o=jr(t);if(o.length===0)throw new Error(`the screen '${e}' has a path`);let i=u=>u.map(l=>l.startsWith(":")?":":l).join("/");for(let u of we){if(u.id===e)throw new Error(`the screen '${e}' is registered twice`);if(i(u.segments)===i(o))throw new Error(`the screens '${u.id}' and '${e}' have the same path`)}we.push({id:e,segments:o,title:r,render:a})}function at(s){return we.find(e=>e.id===s)??null}var Se="#/",ke=Object.freeze({screen:"home",params:Object.freeze({}),address:Se});function A(s,e={}){let t=at(s);if(!t)throw new Error(`there is no screen '${s}'`);return`#/${t.segments.map(a=>{if(!a.startsWith(":"))return a;let o=e[a.slice(1)];if(typeof o!="string"||!o)throw new Error(`the screen '${s}' needs '${a.slice(1)}'`);return encodeURIComponent(o)}).join("/")}`}function Dr(s){let e;try{e=jr(String(s??"").replace(/^#/,"")).map(t=>decodeURIComponent(t))}catch{return ke}for(let t of we){if(t.segments.length!==e.length)continue;let r={};if(t.segments.every((o,i)=>o.startsWith(":")?(r[o.slice(1)]=e[i],!0):o===e[i]))return{screen:t.id,params:r,address:A(t.id,r)}}return ke}function Br(s=globalThis){let e=new Set,t=()=>Dr(s.location?.hash??""),r=()=>{let a=t();for(let o of[...e])o(a)};return{route:t,open(a){let o=Dr(a);o.address!==t().address&&(s.history.pushState({chorus:!0},"",o.address),r())},back(){if(t().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",Se),r()}},watch(a){let o=i=>a(i);return e.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),e.add(o),o(t()),()=>{e.delete(o),e.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var da=(s,e)=>Je("../",s,e),y=s=>typeof s=="string"&&s?s:null,ua=["playing","paused","buffering"];function Hr(s,e=da){let t=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let a of t){if(!a||typeof a!="object"||typeof a.id!="string"||!a.id)continue;let o=a.now_playing&&typeof a.now_playing=="object"?a.now_playing:null,i=o?y(o.art_url):null;r.set(a.id,{source:y(a.source),nowPlaying:o&&{title:y(o.title),artist:y(o.artist),album:y(o.album),state:ua.includes(o.state)?o.state:null,via:y(o.via),artwork:i?e(a.id,i):null}})}return r}var it={source:null,nowPlaying:null};function ca(s){let e=s&&Array.isArray(s.inputs)?s.inputs:[],t=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return e.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:t.get(r)??r}))}function ha(s){let e=s&&s.sound&&typeof s.sound=="object"?s.sound:{},t=a=>Number.isInteger(a)?a:null,r=a=>typeof a=="boolean"?a:null;return{bass:t(e.bass),treble:t(e.treble),loudness:r(e.loudness),night:r(e.night),speech:r(e.speech)}}function pa(s){let e=s&&typeof s=="object"?s:{},t=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:D(e.limit),effectiveLimit:D(e.effective_limit),quietEnabled:typeof e.quiet_enabled=="boolean"?e.quiet_enabled:null,windows:(Array.isArray(e.quiet)?e.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),start:t(r.start),end:t(r.end),limit:D(r.limit),active:r.active===!0}))}}var fa=["optical","hdmi_arc"];function ma(s){return(s&&Array.isArray(s.input_kinds)?s.input_kinds:[]).filter(t=>t&&typeof t.input=="string"&&t.input&&typeof t.kind=="string").filter(t=>typeof t.tv=="boolean"?t.tv:fa.includes(t.kind)).map(t=>({input:t.input,kind:t.kind}))}function ga(s){let e=s&&s.room_eq&&typeof s.room_eq=="object"?s.room_eq:{},t=r=>typeof r=="number"&&Number.isFinite(r);return{enabled:typeof e.enabled=="boolean"?e.enabled:null,filters:(Array.isArray(e.filters)?e.filters:[]).filter(r=>r&&t(r.freq_hz)&&t(r.gain_db)&&t(r.q)).map(({freq_hz:r,gain_db:a,q:o})=>({freq_hz:r,gain_db:a,q:o})),undo:e.undo===!0}}function lt(s){let e=s&&s.measurement&&typeof s.measurement=="object"?s.measurement:null;if(!e||typeof e.id!="number"||typeof e.zone!="string"||typeof e.state!="string")return null;let t=r=>typeof r=="number"&&Number.isFinite(r)&&r>=0?r:null;return{id:e.id,zone:e.zone,state:e.state,leadMs:t(e.lead_ms),sweepMs:t(e.sweep_ms),tailMs:t(e.tail_ms),reason:typeof e.reason=="string"?e.reason:""}}var Wr=["SL","SR","BL","BR"],va=["FC","LFE",...Wr];function ba(s,e=[],t=[]){let r=s&&typeof s=="object"?s:{},a=h=>Number.isInteger(h)?h:null,o=h=>typeof h=="string"&&h?h:null,i=(Array.isArray(r.bond)?r.bond:[]).map(h=>h?.role),u=(Array.isArray(r.endpoints)?r.endpoints:[]).filter(h=>typeof h=="string"&&h),l=h=>u.some(g=>h.startsWith(`${g}/`)),p=h=>t.some(g=>g.input===h&&g.target===r.id),m=e.filter(({input:h})=>l(h)||p(h)),d=r.bass_management&&typeof r.bass_management=="object"?r.bass_management:{},f=i.some(h=>va.includes(h));return{offered:m.length>0||f,avTrimMs:a(r.av_trim_ms),tvUpmix:o(r.sound&&typeof r.sound=="object"?r.sound.tv_upmix:null),tvInputs:m,set:f,surrounds:i.some(h=>Wr.includes(h)),bass:{crossoverHz:a(d.crossover_hz),subLevel:typeof d.sub_level_db=="number"&&Number.isFinite(d.sub_level_db)?Math.round(d.sub_level_db*100):null,subPolarity:o(d.sub_polarity),active:d.active===!0}}}function dt(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(t=>t&&typeof t.input=="string"&&t.input&&typeof t.target=="string").map(t=>({input:t.input,target:t.target,enabled:t.enabled===!0,stopOnStandby:t.stop_on_standby!==!1,lowLatency:t.low_latency!==!1}))}function qr(s){let e=s&&Array.isArray(s.alarms)?s.alarms:[],t=r=>Number.isInteger(r)&&r>=0?r:0;return e.filter(r=>r&&typeof r.alarm=="string"&&r.alarm&&typeof r.target=="string").map(r=>({id:r.alarm,target:r.target,time:typeof r.time=="string"?r.time:"",days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),source:typeof r.source=="string"?r.source:"",volume:D(r.volume)??0,rampS:t(r.ramp_s),durationMin:t(r.duration_min),enabled:r.enabled===!0,ringing:r.ringing===!0}))}function Vr(s){return(s&&Array.isArray(s.sleep)?s.sleep:[]).filter(t=>t&&typeof t.target=="string"&&t.target).map(t=>({target:t.target,minutes:Number.isInteger(t.minutes)?t.minutes:null,remainingS:Number.isInteger(t.remaining_s)&&t.remaining_s>=0?t.remaining_s:null}))}function Jr(s){return(s&&Array.isArray(s.stored_sources)?s.stored_sources:[]).filter(t=>t&&typeof t.id=="string"&&t.id&&typeof t.kind=="string").map(t=>({id:t.id,kind:t.kind,value:typeof t.value=="string"?t.value:"",name:typeof t.name=="string"&&t.name?t.name:t.id}))}function Gr(s){return!s||!Array.isArray(s.chimes)?null:s.chimes.filter(e=>typeof e=="string"&&e)}function Kr(s){let e=s&&s.soloist&&typeof s.soloist=="object"?s.soloist:null;return e?(Array.isArray(e.receivers)?e.receivers:[]).filter(t=>t&&t.state==="running"&&typeof t.target=="string"&&t.target).map(t=>t.target):null}function ut(s){return(s&&Array.isArray(s.speakers)?s.speakers:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>{let r=t.named===!0,a=y(t.room);return{id:t.id,name:y(t.name)??t.id,named:r,room:a,isNew:!r&&a===null,present:t.present===!0,software:y(t.software),link:y(t.link)??"unknown",key:y(t.key),roles:(Array.isArray(t.roles)?t.roles:[]).filter(o=>typeof o=="string"&&o),firmware:_a(t.firmware)}})}var nt=s=>Number.isSafeInteger(s)&&s>0?s:0;function _a(s){if(!s||typeof s!="object")return null;let e=y(s.reason);return{version:y(s.version),board:y(s.board),slot:Number.isSafeInteger(s.slot)?s.slot:null,state:y(s.state)??"idle",reason:e==="none"?null:e,updateAvailable:s.update_available===!0,image:y(s.image),imageVersion:y(s.image_version),received:nt(s.received),size:nt(s.size)}}function Yr(s){let e=s&&s.firmware&&typeof s.firmware=="object"?s.firmware:null;return e?(Array.isArray(e.images)?e.images:[]).filter(t=>t&&typeof t=="object"&&typeof t.name=="string"&&t.name).map(t=>({name:t.name,version:y(t.version),board:y(t.board),size:nt(t.size),verified:t.verdict==="verified",reason:y(t.reason)})):null}function Xr(s,e){return!s||!s.updateAvailable||!Array.isArray(e)?[]:e.filter(t=>t.verified&&t.board===s.board&&t.version!==s.version)}function Qr(s){return(s&&Array.isArray(s.key_changes)?s.key_changes:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>({id:t.id,pinned:y(t.pinned),offered:y(t.offered)}))}function E(s,e){return(Array.isArray(s)?s:[]).find(t=>t.id===e)??null}function $a(s,e,t,r,a){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let o=Array.isArray(s.bond)?s.bond:[],i=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:D(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:ha(s),limits:pa(s),theater:ba(s,r,a),correction:ga(s),group:i,...i===s.id&&t.get(i)||it,bond:o.filter(u=>u&&typeof u.endpoint=="string"&&typeof u.role=="string").map(u=>({endpoint:u.endpoint,name:e.get(u.endpoint)??u.endpoint,role:u.role}))}}function Zr(s,e){let t=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],a=new Map(r.filter(l=>l&&typeof l.id=="string"&&typeof l.name=="string"&&l.name).map(l=>[l.id,l.name])),o=Hr(s,e),i=ma(s),u=dt(s);return t.map(l=>$a(l,a,o,i,u)).filter(Boolean)}function D(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function ya(s,e){let t=new Map(Zr(s,e).map(d=>[d.id,d.name])),r=Hr(s,e),a=d=>({id:d,name:t.get(d)??d}),o=d=>Array.isArray(d)?d:[],i=d=>o(d).filter(f=>typeof f=="string"&&f).map(a),u=d=>d&&typeof d=="object"&&typeof d.id=="string"&&d.id,l=o(s?.groups).filter(u),p=o(s?.saved_groups).filter(u),m=new Set(p.map(d=>d.id));return[...p.map(d=>{let f=l.find(h=>h.id===d.id);return{id:d.id,name:typeof d.name=="string"&&d.name?d.name:d.id,kind:"saved",active:d.active===!0,defined:i(d.zones),rooms:f?i(f.zones):[],volume:f?D(f.volume):null,...f&&r.get(d.id)||it}}),...l.filter(d=>d.kind==="live"&&!m.has(d.id)).map(d=>{let f=i(d.zones);return{id:d.id,name:f.map(h=>h.name).join(" + ")||d.id,kind:"live",active:null,defined:null,rooms:f,volume:D(d.volume),...r.get(d.id)??it}})]}var ot=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function es(s){let e=new Set,t=null,r=[],a=[],o=[],i="connecting",u=!1,l=null,p=()=>({state:t,rooms:r,groups:a,inputs:o,status:i}),m=()=>{let _=p();for(let w of[...e])w(_)},d=_=>{t=_,r=Zr(_,s.artwork),a=ya(_,s.artwork),o=ca(_)};function f(){l||(l=s.events({onState(_){ot(_)&&(u=!0,d(_),m())},onStatus(_){i!==_&&(i=_,m())}}),s.state().then(_=>{u||!ot(_)||(d(_),m())},()=>{}))}function h(){l?.(),l=null}async function g(_){let w=await s.command(_);return w.signedOut&&i!=="signed-out"&&(i="signed-out",m()),w.ok&&ot(w.state)&&(!t||w.state.serial>t.serial)&&(d(w.state),m()),w}async function v(_,w,x){let M=await s.roomFit(_,w,x);return M.signedOut&&i!=="signed-out"&&(i="signed-out",m()),M}function k(_){return e.add(_),_(p()),()=>e.delete(_)}return{start:f,stop:h,command:g,roomFit:v,subscribe:k,view:p}}var xe=s=>`alarm:${s}`,ts="alarms:draft",rs=s=>`stored:${s}`,ss="stored:draft:",as=s=>`sleep:${s}`,os="sleep:draft:",ct={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},Ae={url:"Stream URL",spotify:"Spotify URI"},wa=Object.freeze({alarm:"",target:"",time:"07:00",days:Object.freeze(["mon","tue","wed","thu","fri"]),source:"",volume:300,rampS:30,durationMin:60,enabled:!0}),ka=Object.freeze({id:"",name:"",kind:"url",value:""}),Sa=Object.freeze({target:"",minutes:30}),ht=s=>`${Math.round(s/10)}%`,xa=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),is=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function Aa(s){let e=Math.max(0,Math.floor(s)),t=Math.floor(e/3600),r=Math.floor(e%3600/60);return t>0?`${t} h ${r} min left`:r>0?`${r} min ${e%60} s left`:`${e} s left`}var ft=class extends b{static properties={known:{type:Boolean},heard:{attribute:!1},alarms:{attribute:!1},stored:{attribute:!1},sleep:{attribute:!1},chimes:{attribute:!1},receivers:{attribute:!1},inputs:{attribute:!1},rooms:{attribute:!1},savedGroups:{attribute:!1},formedGroups:{attribute:!1},refusals:{attribute:!1},refusalFields:{attribute:!1},_alarm:{state:!0},_source:{state:!0},_timer:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3,
    h4 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3,
    h4 {
      padding-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    .draft {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-ringing] {
      border-color: var(--accent);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    button,
    select,
    input:not([type="range"]) {
      box-sizing: border-box;
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    select,
    input[type="text"],
    input[type="url"] {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    input:focus-visible,
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [data-value="ringing"] {
      color: var(--accent);
    }
    [data-fallback],
    [data-unavailable] {
      color: var(--warn);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.known=!1,this.heard=null,this.alarms=[],this.stored=[],this.sleep=[],this.chimes=null,this.receivers=null,this.inputs=[],this.rooms=[],this.savedGroups=[],this.formedGroups=[],this.refusals={},this.refusalFields={},this._alarm={...wa},this._source={...ka},this._timer={...Sa},this.clock=()=>globalThis.performance.now(),this._heardAt=0,this._ticker=null}disconnectedCallback(){super.disconnectedCallback(),this._tickEvery(!1)}willUpdate(e){e.has("heard")&&(this._heardAt=this.clock())}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-holds]")){let t=e.dataset.holds;e.value!==t&&(e.value=t)}this._tickEvery(this.isConnected&&(this.sleep??[]).some(e=>e.remainingS!==null))}_tickEvery(e){e!==(this._ticker!==null)&&(e?this._ticker=globalThis.setInterval(()=>this.tick(),1e3):(globalThis.clearInterval(this._ticker),this._ticker=null))}tick(){this.requestUpdate()}_left(e){let t=Math.floor(Math.max(0,this.clock()-this._heardAt)/1e3);return Math.max(0,e.remainingS-t)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:e,body:t},bubbles:!0,composed:!0}))}_refusal(e){let t=this.refusals?.[e]??"";if(!t)return n`<p role="alert"></p>`;let r=this.refusalFields?.[e]??"";return n`<p role="alert" data-refusal-field=${r||c}>Refused${r?` (${r})`:""}: ${t}</p>`}_place(e){return[...this.rooms??[],...this.savedGroups??[],...this.formedGroups??[]].find(r=>r.id===e)?.name??e}_storedOf(e){return e.startsWith("stored:")?(this.stored??[]).find(t=>t.id===e.slice(7))??null:null}_sourceName(e){if(e.startsWith("chime:"))return`Chime: ${e.slice(6)}`;if(e.startsWith("line-in:"))return`Input: ${(this.inputs??[]).find(a=>a.source===e)?.label??e.slice(8)}`;let t=this._storedOf(e);return t?`${Ae[t.kind]??t.kind}: ${t.name}`:e}_unplayable(e,t){if(e.startsWith("chime:"))return this.chimes!==null&&!this.chimes.includes(e.slice(6))?`This server has no chime "${e.slice(6)}".`:"";if(e.startsWith("line-in:"))return(this.inputs??[]).some(r=>r.source===e)?"":`The input ${e.slice(8)} is not offered now: its speaker is not connected.`;if(e.startsWith("stored:")){let r=this._storedOf(e);if(!r)return`This server has no stored source "${e.slice(7)}".`;if(r.kind!=="spotify")return"";if(this.receivers===null)return"This server runs no Spotify receiver.";let a=(this.savedGroups??[]).some(o=>o.id===t);return this.receivers.includes(`${a?"group":"room"}:${t}`)?"":`No Spotify receiver is running for ${this._place(t)}.`}return"This is not a source an alarm plays."}_alarmOf(e){return(this.alarms??[]).find(t=>t.id===e)??null}_sendable(e,t={}){return qe({...e,alarm:e.id,...t})}_onSwitch(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);t&&this._ask(xe(t.id),this._sendable(t,{enabled:!t.enabled}))}_onStop(e){let t=e.currentTarget.dataset.alarm;this._ask(xe(t),ir(t))}_onDelete(e){let t=e.currentTarget.dataset.alarm;this._ask(xe(t),or(t))}_onEdit(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);if(!t)return;let{id:r,ringing:a,...o}=t;this._alarm={alarm:r,...o}}_alarmRow(e){let t=e.days.length===0?"once":O.filter(o=>e.days.includes(o)).map(o=>ct[o][0]).join(" "),r=e.durationMin===0?"until stopped":`for ${e.durationMin} min`,a=this._unplayable(e.source,e.target);return n`
      <li data-alarm=${e.id} ?data-ringing=${e.ringing}>
        <h4>${e.id}</h4>
        <p data-value="when">${e.time}, ${t}</p>
        <p data-value="what">
          ${this._sourceName(e.source)} in ${this._place(e.target)}, to ${ht(e.volume)} over ${e.rampS} s,
          ${r}
        </p>
        ${a?n`<p data-fallback>${a} The alarm rings the bell chime instead.</p>`:c}
        <div class="row">
          <button
            type="button"
            data-alarm=${e.id}
            aria-label="Alarm ${e.id}"
            aria-pressed=${e.enabled?"true":"false"}
            @click=${this._onSwitch}
          >
            Alarm
          </button>
          <span data-value="enabled">${e.enabled?"On":"Off"}</span>
          ${e.ringing?n`<span data-value="ringing">Ringing now</span>
                <button type="button" data-alarm=${e.id} aria-label="Stop alarm ${e.id}" @click=${this._onStop}>
                  Stop
                </button>`:c}
          <button type="button" data-alarm=${e.id} aria-label="Edit alarm ${e.id}" @click=${this._onEdit}>Edit</button>
          <button type="button" data-alarm=${e.id} aria-label="Delete alarm ${e.id}" @click=${this._onDelete}>
            Delete
          </button>
        </div>
        ${this._refusal(xe(e.id))}
      </li>
    `}_offeredSources(){let e=t=>(this.stored??[]).filter(r=>r.kind===t).map(r=>({value:`stored:${r.id}`,name:r.name}));return[{kind:"chime",label:"Chimes",options:(this.chimes??[]).map(t=>({value:`chime:${t}`,name:t}))},{kind:"line-in",label:"Inputs",options:(this.inputs??[]).map(t=>({value:t.source,name:t.label}))},{kind:"url",label:"Stored stream URLs",options:e("url")},{kind:"spotify",label:"Stored Spotify URIs",options:e("spotify")}]}_alarmDraft(){let e=this._alarm,t=[...this.rooms??[],...this.savedGroups??[]],r=this._offeredSources().flatMap(a=>a.options)[0];return{...e,target:e.target||(t[0]?.id??""),source:e.source||(r?.value??"")}}_setAlarm(e){this._alarm={...this._alarm,...e}}_onAlarmText(e){this._setAlarm({alarm:e.target.value.trim()})}_onAlarmChoice(e){this._setAlarm({[e.target.dataset.field]:e.target.value})}_onAlarmTime(e){if(!xa(e.target.value)){e.target.value=this._alarm.time;return}this._setAlarm({time:e.target.value})}_onAlarmDay(e){let t=e.currentTarget.dataset.day,r=this._alarm.days.includes(t)?this._alarm.days.filter(a=>a!==t):O.filter(a=>a===t||this._alarm.days.includes(a));this._setAlarm({days:r})}_onAlarmVolume(e){this._setAlarm({volume:Number(e.target.value)})}_onAlarmCount(e){let{field:t,max:r}=e.target.dataset,a=is(e.target.value,Number(r));e.target.value=String(a),this._setAlarm({[t]:a})}_onAlarmEnabled(){this._setAlarm({enabled:!this._alarm.enabled})}_onSave(){this._ask(ts,qe(this._alarmDraft()))}_kindNotes(e){let t=[];this.chimes===null&&t.push(["chime","This server does not say which chimes it has, so none is offered here."]),(this.inputs??[]).length===0&&t.push(["line-in","No input is offered now: no speaker with a line-in is connected."]);let r=new Set((this.stored??[]).map(i=>i.kind));r.has("url")||t.push(["url","No stream URL is stored: add one under Stored sources."]),r.has("spotify")?this.receivers===null&&t.push(["spotify","This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]):t.push(["spotify","No Spotify URI is stored: add one under Stored sources."]);let a=e.source?this._unplayable(e.source,e.target):"",o=this._storedOf(e.source)?.kind==="spotify"?"spotify":"chosen";return a&&!(o==="spotify"&&this.receivers===null)&&t.push([o,`${a} The alarm would ring the bell chime instead.`]),t.map(([i,u])=>n`<p data-unavailable=${i}>${u}</p>`)}_alarmForm(){let e=this._alarmDraft(),t=this.rooms??[],r=this.savedGroups??[],a=[...t,...r],o=this._offeredSources(),i=d=>n`<option value=${d.value}>${d.name}</option>`,u=d=>n`<option value=${d.id}>${d.name}</option>`,l=o.some(d=>d.options.some(f=>f.value===e.source)),p=this._alarmOf(e.alarm)!==null,m=e.alarm!==""&&e.target!==""&&e.source!=="";return n`
      <div class="draft" data-draft="alarm">
        <div class="row">
          <label for="alarm-name">Name</label>
          <input
            id="alarm-name"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            .value=${e.alarm}
            aria-label="Alarm name"
            @input=${this._onAlarmText}
          />
          <p>Lower-case letters, digits and "-". An alarm with this name is replaced.</p>
        </div>
        <div class="row">
          <label for="alarm-target">Rings in</label>
          <select id="alarm-target" data-field="target" data-holds=${e.target} aria-label="Alarm target" @change=${this._onAlarmChoice}>
            ${a.some(d=>d.id===e.target)||!e.target?c:n`<option value=${e.target}>${e.target} (not on this server now)</option>`}
            ${t.length===0?c:n`<optgroup label="Rooms">${t.map(u)}</optgroup>`}
            ${r.length===0?c:n`<optgroup label="Saved groups">${r.map(u)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${e.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${O.map(d=>n`<button
                type="button"
                data-day=${d}
                aria-label="${ct[d][1]}, the alarm"
                aria-pressed=${e.days.includes(d)?"true":"false"}
                @click=${this._onAlarmDay}
              >
                ${ct[d][0]}
              </button>`)}
          <p data-value="days">${e.days.length===0?"No day: it rings once, at the next such time.":"It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${e.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${l||!e.source?c:n`<option value=${e.source}>${e.source} (not on this server now)</option>`}
            ${o.map(d=>d.options.length===0?c:n`<optgroup label=${d.label} data-kind=${d.kind}>${d.options.map(i)}</optgroup>`)}
          </select>
        </div>
        ${this._kindNotes(e)}
        <div class="row">
          <label for="alarm-volume">Volume</label>
          <input
            id="alarm-volume"
            type="range"
            min="0"
            max="1000"
            step="1"
            .value=${String(e.volume)}
            aria-label="Alarm volume"
            aria-valuetext=${ht(e.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${ht(e.volume)}</span>
        </div>
        <div class="row">
          <label for="alarm-ramp">Rises over, seconds</label>
          <input
            id="alarm-ramp"
            type="number"
            min="0"
            max=${de}
            step="1"
            data-field="rampS"
            data-max=${de}
            .value=${String(e.rampS)}
            aria-label="Alarm ramp, seconds"
            @change=${this._onAlarmCount}
          />
          <label for="alarm-duration">Plays for, minutes</label>
          <input
            id="alarm-duration"
            type="number"
            min="0"
            max=${ue}
            step="1"
            data-field="durationMin"
            data-max=${ue}
            .value=${String(e.durationMin)}
            aria-label="Alarm duration, minutes"
            @change=${this._onAlarmCount}
          />
          <p>0 minutes plays until it is stopped.</p>
        </div>
        <div class="row">
          <button type="button" aria-label="Alarm switched on" aria-pressed=${e.enabled?"true":"false"} @click=${this._onAlarmEnabled}>
            Switched on
          </button>
          <button type="button" aria-label="Save alarm" ?disabled=${!m} @click=${this._onSave}>Save alarm</button>
          <p>${p?`Saving replaces the alarm "${e.alarm}".`:"Nothing is sent until it is saved."}</p>
        </div>
        ${this._refusal(ts)}
      </div>
    `}_onForget(e){let t=e.currentTarget.dataset.stored;this._ask(rs(t),lr(t))}_onSourceField(e){this._source={...this._source,[e.target.dataset.field]:e.target.value.trim()}}_onStore(){let{id:e,kind:t,value:r,name:a}=this._source;this._ask(ss,nr(e,t,r,a||e))}_storedRow(e){return n`
      <li data-stored=${e.id}>
        <h4>${e.name}</h4>
        <p><span data-value="kind">${Ae[e.kind]??e.kind}</span>, <span data-id>${e.id}</span></p>
        <p data-value="value">${e.value}</p>
        <div class="row">
          <button type="button" data-stored=${e.id} aria-label="Forget stored source ${e.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(rs(e.id))}
      </li>
    `}_storedForm(){let e=this._source,t=e.kind==="spotify";return n`
      <div class="draft" data-draft="stored">
        <div class="row">
          <label for="stored-kind">Kind</label>
          <select id="stored-kind" data-field="kind" data-holds=${e.kind} aria-label="Stored source kind" @change=${this._onSourceField}>
            <option value="url">${Ae.url}</option>
            <option value="spotify">${Ae.spotify}</option>
          </select>
          <label for="stored-id">Id</label>
          <input
            id="stored-id"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="id"
            .value=${e.id}
            aria-label="Stored source id"
            @input=${this._onSourceField}
          />
        </div>
        <div class="row">
          <label for="stored-name">Name</label>
          <input id="stored-name" type="text" data-field="name" .value=${e.name} aria-label="Stored source name" @input=${this._onSourceField} />
        </div>
        <div class="row">
          <label for="stored-value">${t?"Spotify URI":"Address"}</label>
          <input
            id="stored-value"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="value"
            .value=${e.value}
            placeholder=${t?"spotify:playlist:...":"https://..."}
            aria-label="Stored source address"
            @input=${this._onSourceField}
          />
        </div>
        <p>
          ${t?"A playlist, an album, a track or an episode, as the Spotify app shares it: spotify:playlist:<id>.":"An http:// or https:// address of a stream. Everyone who can open this app can read it: do not store one with a password in it."}
        </p>
        <div class="row">
          <button type="button" aria-label="Store source" ?disabled=${!e.id||!e.value} @click=${this._onStore}>
            Store source
          </button>
          <p>A source stored under an id already taken replaces it.</p>
        </div>
        ${this._refusal(ss)}
      </div>
    `}_sleepTargets(){return[...this.rooms??[],...this.formedGroups??[]]}_onCancel(e){let t=e.currentTarget.dataset.target;this._ask(as(t),Ve(t,0))}_onSleepTarget(e){this._timer={...this._timer,target:e.target.value}}_onSleepMinutes(e){let t=is(e.target.value,ce);e.target.value=String(t),this._timer={...this._timer,minutes:t}}_onSleep(){let e=this._timer.target||(this._sleepTargets()[0]?.id??"");e&&this._ask(os,Ve(e,this._timer.minutes))}_sleepRow(e){let t=this._place(e.target),r=e.remainingS===null?`${e.minutes??"?"} min asked for`:Aa(this._left(e));return n`
      <li data-sleep=${e.target}>
        <h4>${t}</h4>
        <div class="row">
          <span class="figure" data-value="left">${r}</span>
          <button type="button" data-target=${e.target} aria-label="Cancel sleep timer for ${t}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(as(e.target))}
      </li>
    `}_sleepForm(){let e=this.rooms??[],t=this.formedGroups??[],r=this._timer.target||(this._sleepTargets()[0]?.id??""),a=o=>n`<option value=${o.id}>${o.name}</option>`;return n`
      <div class="draft" data-draft="sleep">
        <div class="row">
          <label for="sleep-target">For</label>
          <select id="sleep-target" data-holds=${r} aria-label="Sleep timer target" @change=${this._onSleepTarget}>
            ${e.length===0?c:n`<optgroup label="Rooms">${e.map(a)}</optgroup>`}
            ${t.length===0?c:n`<optgroup label="Groups playing now">${t.map(a)}</optgroup>`}
          </select>
          <label for="sleep-minutes">Minutes</label>
          <input
            id="sleep-minutes"
            type="number"
            min="0"
            max=${ce}
            step="1"
            .value=${String(this._timer.minutes)}
            aria-label="Sleep timer minutes"
            @change=${this._onSleepMinutes}
          />
          <button type="button" aria-label="Start sleep timer" ?disabled=${!r} @click=${this._onSleep}>Start</button>
        </div>
        <p>It fades the room out and stops it when the time is up. 0 minutes cancels the timer it has.</p>
        ${this._refusal(os)}
      </div>
    `}render(){if(!this.known)return n`<p role="status" data-missing>Reading this server's alarms.</p>`;let e=this.alarms??[],t=this.stored??[],r=this.sleep??[];return n`
      <h2>Alarms</h2>
      <p>An alarm rings in its room or its saved group on the server's own clock, and rises from silence to its volume.</p>
      ${e.length===0?n`<p role="status" data-none="alarms">This server has no alarm.</p>`:n`<ul aria-label="Alarms">
            ${e.map(a=>this._alarmRow(a))}
          </ul>`}
      <h3>Set an alarm</h3>
      ${this._alarmForm()}

      <h2>Stored sources</h2>
      <p>A stream URL or a Spotify URI the server keeps for an alarm to play.</p>
      ${t.length===0?n`<p role="status" data-none="stored">This server has no stored source.</p>`:n`<ul aria-label="Stored sources">
            ${t.map(a=>this._storedRow(a))}
          </ul>`}
      <h3>Store a source</h3>
      ${this._storedForm()}

      <h2>Sleep timers</h2>
      ${r.length===0?n`<p role="status" data-none="sleep">No sleep timer is running.</p>`:n`<ul aria-label="Sleep timers">
            ${r.map(a=>this._sleepRow(a))}
          </ul>`}
      <h3>Start a sleep timer</h3>
      ${this._sleepForm()}
    `}};customElements.define("chorus-alarms",ft);var mt="alarms",pt=s=>s.map(({id:e,name:t})=>({id:e,name:t}));S({id:mt,path:"alarms",title:()=>"Alarms and sleep timers",render:(s,{view:e,refusals:t,refusalFields:r})=>{let a=e.groups??[];return n`
      <chorus-alarms
        .known=${e.state!==null}
        .heard=${e.state}
        .alarms=${qr(e.state)}
        .stored=${Jr(e.state)}
        .sleep=${Vr(e.state)}
        .chimes=${Gr(e.state)}
        .receivers=${Kr(e.state)}
        .inputs=${e.inputs??[]}
        .rooms=${pt(e.rooms)}
        .savedGroups=${pt(a.filter(o=>o.kind==="saved"))}
        .formedGroups=${pt(a.filter(o=>o.rooms.length>0))}
        .refusals=${t}
        .refusalFields=${r}
      ></chorus-alarms>
    `}});function ae(s,e){return e.find(t=>t.id===s.group&&t.rooms.some(r=>r.id===s.id))??null}function ns(s,e,t){if(!s||!e)return null;let r=ae(s,t);return e.kind==="alone"?r?ie(s.id):null:typeof e.id!="string"||!e.id?null:e.kind==="group"?r&&r.id===e.id?null:Ue(s.id,e.id):e.kind==="room"?e.id===s.id||r&&r.rooms.some(a=>a.id===e.id)?null:Ue(s.id,e.id):null}var gt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function ls(s){if(s==="alone")return{kind:"alone"};let e=String(s).indexOf(":");if(e<1)return null;let t=s.slice(0,e),r=s.slice(e+1);return(t==="room"||t==="group")&&r?{kind:t,id:r}:null}function ds(s,e){let t=ae(s,e);return t?gt({kind:"group",id:t.id}):"alone"}function us(s,e,t){return[{value:"alone",label:"Alone"},...t.map(r=>({value:gt({kind:"group",id:r.id}),label:r.name})),...e.filter(r=>r.id!==s.id&&!ae(r,t)).map(r=>({value:gt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var cs=s=>`autoplay:${s}`;function bt(s){let e=dt(s.state),t=i=>e.find(u=>u.input===i)??null,r=(s.inputs??[]).map(i=>({input:i.id,label:i.label,offered:!0,rule:t(i.id)})),a=new Set(r.map(i=>i.input)),o=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(i=>i&&typeof i.input=="string"&&typeof i.name=="string"&&i.name).map(i=>[i.input,i.name]));return[...r,...e.filter(i=>!a.has(i.input)).map(i=>({input:i.input,label:o.get(i.input)??i.input,offered:!1,rule:i}))]}var Ea={optical:"Optical",hdmi_arc:"HDMI ARC"},Ta=[{field:"stopOnStandby",name:"Stop on standby",says:"The TV going to standby stops it at once, with no hold"},{field:"lowLatency",name:"Low latency",says:"Played in low-latency mode when it plays in one wired room; off keeps it on the ordinary path"}],vt=class extends b{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1},tv:{type:Boolean,reflect:!0},home:{attribute:!1}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    :host([tv]) {
      padding: var(--reset-margin);
      border: none;
      background: none;
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    button,
    select {
      box-sizing: border-box;
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    select {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={},this.tv=!1,this.home=null}_row(e){return(this.rows??[]).find(t=>t.input===e)??null}_ask(e,t,r,a=e.rule??{}){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:cs(e.input),body:ar(e.input,t,r,a)},bubbles:!0,composed:!0}))}_onSwitch(e){let t=this._row(e.currentTarget.dataset.input);t&&(t.rule?this._ask(t,t.rule.target,!t.rule.enabled):this.home&&this._ask(t,this.home.id,!0))}_onOption(e){let t=this._row(e.currentTarget.dataset.input),r=e.currentTarget.dataset.option,a=t?.rule?.target??this.home?.id;if(!t||!a)return;let o={stopOnStandby:t.rule?.stopOnStandby??!0,lowLatency:t.rule?.lowLatency??!0};this._ask(t,a,t.rule?.enabled??!1,{...o,[r]:!o[r]})}_onTarget(e){let t=this._row(e.target.dataset.input),r=e.target.value,a=t?.rule?.target??"";e.target.value=a,!(!t||!r||r===a)&&this._ask(t,r,t.rule?.enabled??!1)}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-input]")){let t=this._row(e.dataset.input)?.rule?.target??"";e.value!==t&&(e.value=t)}}_targets(e){let t=this.rooms??[],r=this.groups??[],a=!e||[...t,...r].some(i=>i.id===e.target),o=i=>n`<option value=${i.id} ?selected=${e?.target===i.id}>${i.name}</option>`;return n`
      ${e?c:n`<option value="" selected>Nowhere yet</option>`}
      ${a?c:n`<option value=${e.target} selected>${e.target} (not on this server now)</option>`}
      ${t.length===0?c:n`<optgroup label="Rooms">${t.map(o)}</optgroup>`}
      ${r.length===0?c:n`<optgroup label="Saved groups">${r.map(o)}</optgroup>`}
    `}_input(e){let{input:t,label:r,offered:a,rule:o}=e,i=this.refusals?.[cs(t)]??"",u=!o&&!!this.home,l=o?o.enabled?"On":"Off":u?`Off: switching it on plays it in ${this.home.name}`:"Choose where it plays, then switch it on";return n`
      <li data-input=${t}>
        <h3>${r}</h3>
        ${r===t?c:n`<p data-id>${t}</p>`}
        ${e.kind?n`<p data-kind>${Ea[e.kind]??e.kind}</p>`:c}
        ${a?c:n`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${t}
            aria-label="Autoplay for ${r}"
            aria-pressed=${o?.enabled?"true":"false"}
            ?disabled=${!o&&!u}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${l}</span>
        </div>
        <div class="row">
          <label for="target-${t}">Plays in</label>
          <select id="target-${t}" data-input=${t} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(o)}
          </select>
        </div>
        ${this.tv?Ta.map(p=>this._option(e,p,!o&&!u)):c}
        <p role="alert">${i?`Refused: ${i}`:c}</p>
      </li>
    `}_option({input:e,label:t,rule:r},{field:a,name:o,says:i},u){let l=r?.[a]??!0;return n`
      <div class="row">
        <button
          type="button"
          data-input=${e}
          data-option=${a}
          aria-label="${o} for ${t}"
          aria-pressed=${l?"true":"false"}
          ?disabled=${u}
          @click=${this._onOption}
        >
          ${o}
        </button>
        <span data-value=${a}>${l?"On":"Off"}</span>
        <p>${i}</p>
      </div>
    `}render(){if(this.rows===null)return n`<p role="status" data-missing>Reading this server's inputs.</p>`;let e=this.tv?"This room has no TV input now.":"This server offers no input now, and has no autoplay rule.";return n`
      ${this.tv?n`<h3>TV autoplay</h3>`:n`<h2>Autoplay</h2>`}
      <p>
        ${this.tv?"A TV input with a rule that is on plays when the TV's signal arrives.":"An input with a rule that is on plays in its room or its group when its signal arrives."}
      </p>
      ${this.rows.length===0?n`<p role="status" data-none>${e}</p>`:n`<ul aria-label=${this.tv?"TV inputs":"Inputs"}>
            ${this.rows.map(t=>this._input(t))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",vt);var _t="autoplay",hs=s=>s.map(({id:e,name:t})=>({id:e,name:t}));S({id:_t,path:"autoplay",title:()=>"Autoplay",render:(s,{view:e,refusals:t})=>n`
    <chorus-autoplay
      .rows=${e.state===null?null:bt(e)}
      .rooms=${hs(e.rooms)}
      .groups=${hs((e.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${t}
    ></chorus-autoplay>
  `});var ps={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},Ee=s=>(...e)=>({_$litDirective$:s,values:e}),W=class{constructor(e){}get _$AU(){return this._$AM._$AU}_$AT(e,t,r){this._$Ct=e,this._$AM=t,this._$Ci=r}_$AS(e,t){return this.update(e,t)}update(e,t){return this.render(...t)}};var{I:Oa}=Lr,fs=s=>s;var ms=()=>document.createComment(""),q=(s,e,t)=>{let r=s._$AA.parentNode,a=e===void 0?s._$AB:e._$AA;if(t===void 0){let o=r.insertBefore(ms(),a),i=r.insertBefore(ms(),a);t=new Oa(o,i,s,s.options)}else{let o=t._$AB.nextSibling,i=t._$AM,u=i!==s;if(u){let l;t._$AQ?.(s),t._$AM=s,t._$AP!==void 0&&(l=s._$AU)!==i._$AU&&t._$AP(l)}if(o!==a||u){let l=t._$AA;for(;l!==o;){let p=fs(l).nextSibling;fs(r).insertBefore(l,a),l=p}}}return t},z=(s,e,t=s)=>(s._$AI(e,t),s),Ca={},Te=(s,e=Ca)=>s._$AH=e,gs=s=>s._$AH,Oe=s=>{s._$AR(),s._$AA.remove()};var vs=(s,e,t)=>{let r=new Map;for(let a=e;a<=t;a++)r.set(s[a],a);return r},Ce=Ee(class extends W{constructor(s){if(super(s),s.type!==ps.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,e,t){let r;t===void 0?t=e:e!==void 0&&(r=e);let a=[],o=[],i=0;for(let u of s)a[i]=r?r(u,i):i,o[i]=t(u,i),i++;return{values:o,keys:a}}render(s,e,t){return this.dt(s,e,t).values}update(s,[e,t,r]){let a=gs(s),{values:o,keys:i}=this.dt(e,t,r);if(!Array.isArray(a))return this.ut=i,o;let u=this.ut??=[],l=[],p,m,d=0,f=a.length-1,h=0,g=o.length-1;for(;d<=f&&h<=g;)if(a[d]===null)d++;else if(a[f]===null)f--;else if(u[d]===i[h])l[h]=z(a[d],o[h]),d++,h++;else if(u[f]===i[g])l[g]=z(a[f],o[g]),f--,g--;else if(u[d]===i[g])l[g]=z(a[d],o[g]),q(s,l[g+1],a[d]),d++,g--;else if(u[f]===i[h])l[h]=z(a[f],o[h]),q(s,a[d],a[f]),f--,h++;else if(p===void 0&&(p=vs(i,h,g),m=vs(u,d,f)),p.has(u[d]))if(p.has(u[f])){let v=m.get(i[h]),k=v!==void 0?a[v]:null;if(k===null){let _=q(s,a[d]);z(_,o[h]),l[h]=_}else l[h]=z(k,o[h]),q(s,a[d],k),a[v]=null;h++}else Oe(a[f]),f--;else Oe(a[d]),d++;for(;h<=g;){let v=q(s,l[g+1]);z(v,o[h]),l[h++]=v}for(;d<=f;){let v=a[d++];v!==null&&Oe(v)}return this.ut=i,Te(s,l),N}});var bs=Ee(class extends W{constructor(){super(...arguments),this.key=c}render(s,e){return this.key=s,e}update(s,[e,t]){return e!==this.key&&(Te(s),this.key=e),t}});var Ra={playing:"Playing",paused:"Paused",buffering:"Buffering"};function Na(s,e=[]){if(!s)return"Unavailable";let t=e.find(i=>i.source===s);if(t)return t.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...a]=s.split(":"),o=a.join(":");return r==="line-in"&&o?`Input ${o}`:r==="player"&&o?`Network player ${o}`:r==="chime"&&o?`Chime ${o}`:r==="soloist"&&o?"Spotify":s}var $t=class extends b{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
    :host {
      display: block;
    }
    .now {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    img,
    .placeholder {
      flex: none;
      width: var(--artwork-size);
      height: var(--artwork-size);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--artwork-radius);
      background: var(--bg);
      object-fit: cover;
    }
    .placeholder {
      display: flex;
      align-items: center;
      justify-content: center;
      box-sizing: border-box;
      color: var(--muted);
      font-size: var(--heading-size);
    }
    .words {
      flex: 1;
      min-width: var(--shrink-min);
    }
    p,
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    p[data-title] {
      color: var(--fg);
      font-size: var(--body-size);
    }
    ul {
      display: flex;
      flex-wrap: wrap;
      gap: var(--surface-gap);
    }
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(e){this._failed=e.target.getAttribute("src")}_onInput(e){e.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:Xt(this.target,e.source)},bubbles:!0,composed:!0}))}_artwork(e){let t=n`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!e.artwork||e.artwork===this._failed?t:bs(e.artwork,n`<img
        data-artwork="image"
        src=${e.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let e=this.nowPlaying,t=this.inputs??[];return n`
      ${e?n`<div class="now" data-now-playing=${e.state??"unknown"}>
            ${this._artwork(e)}
            <div class="words">
              <p data-title>${e.title??"Unknown title"}</p>
              ${e.artist?n`<p data-artist>${e.artist}</p>`:c}
              ${e.album?n`<p data-album>${e.album}</p>`:c}
              <p data-state>${Ra[e.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${Na(this.source,t)}</p>
      ${this.pick&&t.length>0?n`<ul aria-label="Inputs for ${this.name}">
            ${t.map(r=>n`<li data-input=${r.id}>
                  <button
                    type="button"
                    aria-label="Play ${r.label} in ${this.name}"
                    aria-pressed=${r.source===this.source?"true":"false"}
                    @click=${()=>this._onInput(r)}
                  >
                    ${r.label}
                  </button>
                </li>`)}
          </ul>`:c}
    `}};customElements.define("chorus-playing",$t);var Ma=s=>`${Math.round(s/10)}%`,yt=class extends b{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    p,
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    li,
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    li span:first-child {
      flex: 1;
      color: var(--fg);
    }
    label {
      min-width: var(--label-min-width);
    }
    /* The slider keeps a width a finger can travel: in a narrow card, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      flex-wrap: wrap;
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
    }
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._slider;if(!t||!this.group||this.group.volume===null)return;let r=e.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(t.value=String(this.group.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(Qt(this.group.id,Number(e.target.value)))}_onActivate(){this._ask(ie(this.group.id))}_onRemove(e){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:e.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let e=this.group;return e.kind==="live"?"Live group":e.active?"Saved group, active":e.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let e=this.group,t=new Set(e.rooms.map(o=>o.id)),r=e.defined??[],a=new Set(r.map(o=>o.id));return[...r.map(o=>({...o,playing:t.has(o.id)})),...e.rooms.filter(o=>!a.has(o.id)).map(o=>({...o,playing:!0}))]}render(){let e=this.group;if(!e)return c;let t=e.volume===null?"":Ma(this._dragged??e.volume);return n`
      <h2>${e.name}</h2>
      <p data-kind=${e.kind} data-active=${e.active===null?c:String(e.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${e.name}">
        ${this._listed().map(r=>n`<li data-member=${r.id} data-playing=${String(r.playing)}>
              <span>${r.name}</span>
              ${r.playing?n`<button
                    type="button"
                    aria-label="Remove ${r.name} from ${e.name}"
                    @click=${()=>this._onRemove(r)}
                  >
                    Remove
                  </button>`:n`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${e.source?n`<chorus-playing
            .target=${e.id}
            .name=${e.name}
            .source=${e.source}
            .nowPlaying=${e.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${e.kind==="live"||e.active===!0}
          ></chorus-playing>`:c}
      ${e.kind==="saved"&&!e.active?n`<div class="row">
            <button type="button" aria-label="Group the rooms of ${e.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:c}
      ${e.volume===null?c:n`<div class="row">
            <label for="volume">Group volume</label>
            <input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Group volume for ${e.name}"
              aria-valuetext=${t}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />
            <span class="figure" data-volume>${t}</span>
          </div>`}
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-group-card",yt);var wt=class extends b{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
    :host {
      display: block;
    }
    ul {
      display: flex;
      flex-direction: column;
      gap: var(--surface-gap);
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    [data-drop="alone"] {
      display: flex;
      align-items: center;
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) dashed var(--border);
      border-radius: var(--surface-radius);
    }
    [hidden] {
      display: none;
    }
    [data-over] {
      outline: var(--focus-ring-width) solid var(--accent);
      outline-offset: var(--focus-ring-offset);
    }
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let e=this.groups??[],t=this.over;return n`
      ${this.groups!==null&&e.length===0?n`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${Ce(e,r=>r.id,r=>n`<li
              data-group=${r.id}
              data-drop="group"
              data-drop-id=${r.id}
              ?data-over=${t?.kind==="group"&&t.id===r.id}
            >
              <chorus-group-card
                .group=${r}
                .inputs=${this.inputs}
                .refusal=${this.refusals[r.id]??""}
              ></chorus-group-card>
            </li>`)}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${t?.kind==="alone"}>
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:c}
      </p>
    `}};customElements.define("chorus-groups",wt);var _s=Object.freeze(["phone","desktop"]),za=48,Ia=`(min-width: ${za}em)`;function $s(s,e=globalThis){if(typeof e?.matchMedia!="function")return s("phone"),()=>{};let t=e.matchMedia(Ia),r=()=>s(t.matches?"desktop":"phone");return t.addEventListener("change",r),r(),()=>t.removeEventListener("change",r)}var kt=s=>`limits:${s}`,ys={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},La=Object.freeze({days:O,start:"22:00",end:"07:00",limit:250}),j=s=>`${Math.round(s/10)}%`,ws=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),Pa=({days:s,start:e,end:t,limit:r})=>({days:s,start:e,end:t,limit:r}),St=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      padding-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ol {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    .draft {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-active] {
      border-color: var(--accent);
    }
    /* A slider keeps a width a finger can travel: in a narrow screen, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    input[type="time"] {
      box-sizing: border-box;
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [data-value="active"][data-active] {
      color: var(--accent);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...La},this._held=new Set,this._asked=null,this._unanswered=0}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||t)&&(r.value=r.dataset.server)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:kt(this.room.id),body:e,done:t},bubbles:!0,composed:!0}))}_askWindows(e){let t=this.room.id,r=(this._asked?.room===t?this._asked.windows:this.room.limits.windows).map(Pa);e(r),this._asked={room:t,windows:r},this._unanswered+=1,this._ask(rr(t,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onFocus(e){this._held.add(e.target.dataset.key)}_onBlur(e){let{key:t,server:r}=e.target.dataset;this._held.delete(t),this._release(t),e.target.value=r}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.key]:Number(e.target.value)}}_onLimitChange(e){this._release(e.target.dataset.key),this._ask(tr(this.room.id,Number(e.target.value)))}_onEnabled(){this._ask(sr(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(e){let t=Number(e.target.dataset.window),r=Number(e.target.value);this._release(e.target.dataset.key),this._askWindows(a=>{a[t]={...a[t],limit:r}})}_onWindowTime(e){let{window:t,edge:r,server:a}=e.target.dataset,o=e.target.value;if(!ws(o)){e.target.value=a;return}o!==a&&this._askWindows(i=>{i[Number(t)]={...i[Number(t)],[r]:o}})}_onWindowDay(e){let{window:t,day:r}=e.currentTarget.dataset;this._askWindows(a=>{let o=a[Number(t)],i=o.days.includes(r)?o.days.filter(u=>u!==r):[...o.days,r];a[Number(t)]={...o,days:i}})}_onRemove(e){let t=Number(e.currentTarget.dataset.window);this._askWindows(r=>r.splice(t,1))}_onDraftDay(e){let t=e.currentTarget.dataset.day,r=this._draft.days.includes(t)?this._draft.days.filter(a=>a!==t):O.filter(a=>a===t||this._draft.days.includes(a));this._draft={...this._draft,days:r}}_onDraftTime(e){let t=e.target.dataset.edge;if(!ws(e.target.value)){e.target.value=this._draft[t];return}this._draft={...this._draft,[t]:e.target.value}}_onDraftLimit(e){this._draft={...this._draft,limit:Number(e.target.value)}}_onAdd(){this._askWindows(e=>e.push({...this._draft}))}_days(e,t,r,a){let o=this.room;return n`
      <div class="row" role="group" aria-label="Days of ${t} for ${o.name}">
        ${O.map(i=>n`<button
              type="button"
              data-day=${i}
              data-window=${a??c}
              aria-label="${ys[i][1]}, ${t} for ${o.name}"
              aria-pressed=${e.includes(i)?"true":"false"}
              @click=${r}
            >
              ${ys[i][0]}
            </button>`)}
      </div>
    `}_window(e,t,r){let a=this.room,o=`window ${t+1}`,i=a.limits.quietEnabled!==!1,u=e.active?i?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return n`<li data-window=${t}><p data-value="active">Unavailable</p></li>`;let l=`window-${t}`;return n`
      <li data-window=${t} ?data-active=${e.active}>
        <div class="row">
          <strong>Window ${t+1}</strong>
          <span data-value="active" ?data-active=${e.active&&i}>${u}</span>
        </div>
        ${this._days(e.days,o,this._onWindowDay,t)}
        <div class="row">
          <label for="${l}-start">From</label>
          <input
            id="${l}-start"
            type="time"
            data-key="${l}-start"
            data-window=${t}
            data-edge="start"
            data-server=${e.start}
            aria-label="Start of ${o} for ${a.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
          <label for="${l}-end">Until</label>
          <input
            id="${l}-end"
            type="time"
            data-key="${l}-end"
            data-window=${t}
            data-edge="end"
            data-server=${e.end}
            aria-label="End of ${o} for ${a.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
        </div>
        <div class="row">
          <label for="${l}-limit">Limit</label>
          <input
            id="${l}-limit"
            type="range"
            min="0"
            max="1000"
            step="1"
            data-key="${l}-limit"
            data-window=${t}
            data-server=${e.limit}
            aria-label="Limit of ${o} for ${a.name}"
            aria-valuetext=${j(this._dragged[`${l}-limit`]??e.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${j(this._dragged[`${l}-limit`]??e.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${t} aria-label="Remove ${o} for ${a.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(e){let t=this.room;if(e>=We)return n`<p data-full>A room has at most ${We} windows. Remove one to add another.</p>`;let r=this._draft,a="the new window";return n`
      <div class="draft" data-draft>
        ${this._days(r.days,a,this._onDraftDay)}
        <div class="row">
          <label for="draft-start">From</label>
          <input
            id="draft-start"
            type="time"
            data-edge="start"
            .value=${r.start}
            aria-label="Start of ${a} for ${t.name}"
            @change=${this._onDraftTime}
          />
          <label for="draft-end">Until</label>
          <input
            id="draft-end"
            type="time"
            data-edge="end"
            .value=${r.end}
            aria-label="End of ${a} for ${t.name}"
            @change=${this._onDraftTime}
          />
        </div>
        <div class="row">
          <label for="draft-limit">Limit</label>
          <input
            id="draft-limit"
            type="range"
            min="0"
            max="1000"
            step="1"
            .value=${String(r.limit)}
            aria-label="Limit of ${a} for ${t.name}"
            aria-valuetext=${j(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${j(r.limit)}</span>
        </div>
        <div class="row">
          <button
            type="button"
            aria-label="Add window for ${t.name}"
            ?disabled=${r.days.length===0}
            @click=${this._onAdd}
          >
            Add window
          </button>
          <p>${r.days.length===0?"A window starts on at least one day.":"Nothing is sent until it is added."}</p>
        </div>
      </div>
    `}render(){let e=this.room;if(!e)return n`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let{limit:t,effectiveLimit:r,quietEnabled:a,windows:o}=e.limits,i=o.every(p=>p.start&&p.end&&p.limit!==null&&p.days.length>0),u=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c,l=t===null?"Unavailable":j(this._dragged.limit??t);return n`
      <h2>Volume limits of ${e.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${t===null?c:n`<input
              id="limit"
              type="range"
              min="0"
              max="1000"
              step="1"
              data-key="limit"
              data-server=${t}
              aria-label="Volume limit for ${e.name}"
              aria-valuetext=${l}
              @focus=${this._onFocus}
              @blur=${this._onBlur}
              @input=${this._onSliderInput}
              @change=${this._onLimitChange}
            />`}
        <span class="figure" data-value="limit">${l}</span>
      </div>
      <div class="row">
        <span>Limit in force now</span>
        <span class="figure" data-value="effective">${r===null?"Unavailable":j(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${e.volume===null?"Unavailable":j(e.volume)}</span>
      </div>
      <h3>Quiet hours</h3>
      <div class="row">
        <button
          type="button"
          aria-label="Quiet hours for ${e.name}"
          aria-pressed=${a===!0?"true":"false"}
          ?disabled=${a===null}
          @click=${this._onEnabled}
        >
          Quiet hours
        </button>
        <span data-value="enabled">${a===null?"Unavailable":a?"On":"Off"}</span>
        <p>Off, no window caps the room, and every window is kept.</p>
      </div>
      ${o.length===0?n`<p data-none>This room has no quiet-hours window.</p>`:c}
      <ol aria-label="Quiet-hours windows of ${e.name}">
        ${o.map((p,m)=>this._window(p,m,i))}
      </ol>
      ${i?n`<h3>Add a window</h3>
            ${this._adding(o.length)}`:n`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${u}</p>
    `}};customElements.define("chorus-room-limits",St);var xt="room-limits";S({id:xt,path:"rooms/:room/limits",title:({room:s},e)=>`Volume limits of ${E(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-limits
      .room=${E(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[kt(s)]??""}
      .refusalField=${r[kt(s)]??""}
    ></chorus-room-limits>
  `});var Ss=Object.freeze(["app","kiosk"]),At="chorus.kiosk",ks="1";function Ua(s){let e=new URLSearchParams(s).get("kiosk");return e===null?null:e==="0"||e==="false"?"app":"kiosk"}function xs(s,e){let t=Ua(s);try{if(t==="kiosk")e?.setItem(At,ks);else if(t==="app")e?.removeItem(At);else return e?.getItem(At)===ks?"kiosk":"app"}catch{}return t??"app"}function As(s=globalThis){try{return s.localStorage??null}catch{return null}}var Fa=Object.freeze({echoCancellation:!1,noiseSuppression:!1,autoGainControl:!1,channelCount:1}),Tt=Object.freeze(["echoCancellation","noiseSuppression","autoGainControl"]),Ot=48e3,Da="chorus-capture",ja="assets/capture-worklet-CPTY3ULB.js",Ba=1e3,Re=Object.freeze({insecure:"This page was not opened over HTTPS, so the browser gives it no microphone. Open the app at its https address and try again.",unsupported:"This browser cannot record uncompressed audio in a page, so it cannot measure a room.",denied:"The microphone was not allowed. Allow the microphone for this site in the browser's settings for the page, then try again.",missing:"The browser found no microphone on this device.",busy:"The microphone could not be started. Another app may be using it: close it and try again.",failed:"The microphone could not be opened."}),T=class extends Error{constructor(e,t=""){super(t?`${Re[e]} (${t})`:Re[e]),this.name="CaptureError",this.reason=e}};function Ha(s){return Tt.filter(e=>s?.[e]!==void 0&&s[e]!==!1)}function Wa(s){let e=s?.name??"";return e==="NotAllowedError"||e==="SecurityError"?new T("denied"):e==="NotFoundError"||e==="OverconstrainedError"?new T("missing"):e==="NotReadableError"||e==="AbortError"?new T("busy"):new T("failed",e||String(s?.message??s??""))}var Et=s=>{for(let e of s?.getTracks?.()??[])e.stop()};async function Es(s,e,t){let r=s.AudioContext??s.webkitAudioContext,a=t?new r({sampleRate:t}):new r;try{await a.audioWorklet.addModule(ja);let o=a.createMediaStreamSource(e),i=new s.AudioWorkletNode(a,Da,{numberOfInputs:1,numberOfOutputs:1});return o.connect(i),i.connect(a.destination),a.state==="suspended"&&await a.resume(),{context:a,source:o,node:i}}catch(o){throw await a.close?.().catch?.(()=>{}),o}}async function Cs(s=globalThis){if(s.isSecureContext===!1)throw new T("insecure");let e=s.navigator?.mediaDevices;if(typeof e?.getUserMedia!="function")throw new T("unsupported");if(!(s.AudioContext??s.webkitAudioContext)||!s.AudioWorkletNode)throw new T("unsupported");let t;try{t=await e.getUserMedia({audio:{...Fa},video:!1})}catch(g){throw Wa(g)}let r=t.getAudioTracks?.()[0];if(!r)throw Et(t),new T("missing");let a={...r.getSettings?.()??{}},o;try{try{o=await Es(s,t,Ot)}catch{o=await Es(s,t,0)}}catch(g){throw Et(t),new T("unsupported",String(g?.message??g??""))}let{context:i,source:u,node:l}=o,p=!1,m=!1,d=[],f=null;l.port.onmessage=g=>{let v=g.data;v?.samples&&p&&d.push(v.samples),v?.flushed&&f?.()};function h(){if(!m){m=!0,p=!1,d=[];try{l.port.postMessage("stop"),l.port.onmessage=null,u.disconnect(),l.disconnect()}catch{}Et(t),i.close?.().catch?.(()=>{})}}return{settings:a,kept:Ha(a),sampleRate:i.sampleRate,start(){m||(d=[],p=!0)},async stop(){if(m)return{samples:new Float32Array(0),sampleRate:i.sampleRate};await new Promise(v=>{let k=s.setTimeout(v,Ba);f=()=>{s.clearTimeout(k),v()},l.port.postMessage("flush")}),f=null;let g=qa(d);return h(),{samples:g,sampleRate:i.sampleRate}},close:h}}function qa(s){let e=new Float32Array(s.reduce((r,a)=>r+a.length,0)),t=0;for(let r of s)e.set(r,t),t+=r.length;return e}var Va=16,Ts=8.6;function Os(s){let e=1,t=1;for(let r=1;r<40&&(t*=(s/(2*r))**2,e+=t,!(t<e*1e-12));r+=1);return e}function Ja(s,e,t){if(e===t)return s;let r=e/t,a=Math.min(1,t/e),o=Va/a,i=1/Os(Ts),u=new Float32Array(Math.round(s.length*t/e));for(let l=0;l<u.length;l+=1){let p=l*r,m=Math.max(0,Math.ceil(p-o)),d=Math.min(s.length-1,Math.floor(p+o)),f=0,h=0;for(let g=m;g<=d;g+=1){let v=p-g,k=Math.PI*a*v,_=k===0?1:Math.sin(k)/k,w=v/o,x=_*Os(Ts*Math.sqrt(Math.max(0,1-w*w)))*i;f+=s[g]*x,h+=x}u[l]=h===0?0:f/h}return u}function Rs({samples:s,sampleRate:e}){return Ja(s,e,Ot)}function Ns(s,e=Ot){let t=new Uint8Array(44+s.length*2),r=new DataView(t.buffer),a=(o,i)=>{for(let u=0;u<4;u+=1)t[o+u]=i.charCodeAt(u)};a(0,"RIFF"),r.setUint32(4,36+s.length*2,!0),a(8,"WAVE"),a(12,"fmt "),r.setUint32(16,16,!0),r.setUint16(20,1,!0),r.setUint16(22,1,!0),r.setUint32(24,e,!0),r.setUint32(28,e*2,!0),r.setUint16(32,2,!0),r.setUint16(34,16,!0),a(36,"data"),r.setUint32(40,s.length*2,!0);for(let o=0;o<s.length;o+=1){let i=Math.round(s[o]*32768);r.setInt16(44+o*2,Number.isNaN(i)?0:Math.min(32767,Math.max(-32768,i)),!0)}return t}var Ga=1e3,Ka=5e3,Ya=6500,Ms=Object.freeze({echoCancellation:"Echo cancellation",noiseSuppression:"Noise suppression",autoGainControl:"Automatic gain control"}),Xa=Object.freeze({too_short:"The recording ended before the sweep and the room's answer to it did. Keep this screen open and the phone awake until the sweep has finished, then measure again.",clipped:"The sweep was too loud for the microphone. Turn the room down, or hold the phone further from the speakers, then measure again.",too_quiet:"The sweep was too quiet at the microphone. Turn the room up, move closer and keep the microphone uncovered, then measure again.",too_noisy:"The room was too noisy for the sweep to stand clear of it. Pause music elsewhere, quiet the room (voices, fans, a TV), or turn the room up, then measure again."}),Qa="The recording was not fitted. Measure again.",Za=(s,e)=>`${s>0?"+":""}${s.toFixed(e)}`,zs=s=>`${s.freq_hz} Hz, ${Za(s.gain_db,2)} dB, Q ${s.q.toFixed(3)}`,eo={set:(s,e)=>globalThis.setTimeout(s,e),clear:s=>globalThis.clearTimeout(s)},B=class extends Error{},Ne=class extends Error{},Ct=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},store:{attribute:!1},capture:{attribute:!1},timers:{attribute:!1},_phase:{state:!0},_granted:{state:!0},_message:{state:!0},_fit:{state:!0},_refused:{state:!0},_commandRefusal:{state:!0},_sending:{state:!0},_done:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      margin-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p,
    li,
    dd,
    dt {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul,
    ol,
    dl {
      margin: var(--reset-margin);
      padding-left: var(--surface-pad);
    }
    dl {
      padding-left: var(--reset-margin);
    }
    .setting {
      display: flex;
      flex-wrap: wrap;
      gap: var(--surface-gap);
    }
    dt {
      min-width: var(--label-min-width);
    }
    .figure {
      font-family: var(--face-figure);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [data-kept],
    [data-flag] {
      color: var(--warn);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
    [role="alert"][data-flag] {
      color: var(--warn);
    }
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.store=null,this.capture=Cs,this.timers=eo,this._session=null,this._run=null,this._reset()}_reset(){this._phase="guide",this._granted=null,this._message="",this._fit=null,this._refused=null,this._commandRefusal="",this._sending=!1,this._done=""}_leave(){this._run&&(this._run.left=!0,this._run.wake?.()),this._run=null,this._session?.close(),this._session=null}disconnectedCallback(){super.disconnectedCallback(),this._leave(),this._reset()}willUpdate(e){e.has("roomId")&&e.get("roomId")!==void 0&&(this._leave(),this._reset())}get _busy(){return this._phase==="asking"||this._phase==="recording"||this._phase==="fitting"}async _onMicrophone(){this._leave();let e=this._run={left:!1};this._phase="asking",this._message="",this._fit=null,this._refused=null,this._done="";let t;try{t=await this.capture()}catch(r){if(e.left)return;this._run=null,this._phase="failed",this._message=r instanceof T?r.message:Re.failed;return}if(e.left){t.close();return}this._session=t,this._granted={settings:t.settings??{},kept:t.kept??[],sampleRate:t.sampleRate??null},this._phase="ready"}_wait(e){return new Promise(t=>this.timers.set(t,e))}_sweepEnd(e,t){let r=t?(t.leadMs??0)+(t.sweepMs??0)+(t.tailMs??0):Ya;return new Promise(a=>{let o=()=>{},i=l=>{this.timers.clear(u),o(),e.wake=null,a(l)},u=this.timers.set(()=>i(null),r+Ka);e.wake=()=>i(null),t&&(o=this.store.subscribe(l=>{let p=lt(l.state);p&&p.id===t.id&&p.state!=="playing"&&i(p)}))})}async _onMeasure(){let e=this._run,t=this._session,r=this.room;if(!e||!t||!r||!this.store||this._phase!=="ready")return;let a=this.store,o=()=>{if(e.left)throw new Ne};this._phase="recording";let i=!1,u;try{if(r.correction.enabled!==!1&&r.correction.filters.length>0){let h=await a.command(he(r.id,!1));if(!h.ok)throw new B(`The room's correction could not be switched off for the sweep: ${h.refusal}`);i=!0,o()}t.start();let l=await a.command(vr(r.id));if(o(),!l.ok)throw new B(`The sweep was not played: ${l.refusal}`);let p=lt(l.state),m=await this._sweepEnd(e,p&&p.zone===r.id?p:null);if(o(),!m)throw new B("The server did not say that the sweep ended, so the recording was not used. Measure again.");if(m.state!=="finished")throw new B(`The sweep was called off${m.reason?`: ${m.reason}`:""}. Measure again.`);await this._wait(Ga),o();let d=await t.stop();o(),this._phase="fitting";let f=await a.roomFit(r.id,Ns(Rs(d)),d.sweep??{});o(),u=f.ok?{phase:"proposed",fit:f.fit}:{phase:"refused",refused:{name:f.name??"",words:f.refusal}}}catch(l){l instanceof Ne?u=null:l instanceof B?u={phase:"failed",message:l.message}:u={phase:"failed",message:"The measurement failed in this page. Measure again."}}finally{if(t.close(),this._session===t&&(this._session=null),i){let l=await a.command(he(r.id,!0));!l.ok&&u&&(u={phase:"failed",message:`The room's correction was switched off for the sweep and could not be switched back on: ${l.refusal}`})}}!u||e.left||(this._run=null,this._phase=u.phase,this._fit=u.fit??null,this._refused=u.refused??null,this._message=u.message??"")}async _command(e,t=""){if(!this.store||this._sending)return!1;this._sending=!0,this._commandRefusal="",this._done="";let r=await this.store.command(e);return this._sending=!1,r.ok?(this._done=t,!0):(this._commandRefusal=r.refusal,!1)}async _onApply(){let e=this._fit;!e||!this.room||this._phase!=="proposed"||!await this._command(mr(this.room.id,e.filters),"Applied. Undo puts back what the room had.")||(this._fit=null,this._granted=null,this._phase="guide")}_onDiscard(){this._leave(),this._reset()}_onSwitch(){this._command(he(this.room.id,this.room.correction.enabled!==!0))}_onUndo(){this._command(gr(this.room.id),"Put back.")}_guide(){return n`
      <ol data-guide>
        <li>
          Where: sit or stand where you usually listen and hold this device at ear height, its microphone uncovered
          and nothing between it and the speakers. Hold it still until the sweep has ended.
        </li>
        <li>
          Quiet: pause what plays in other rooms, close the door, and stop anything that hums or talks. A room that is
          too noisy is refused, not guessed at.
        </li>
        <li>
          What plays: half a second of silence, then one rising tone that sweeps from the lowest bass to the highest
          treble in 5 seconds, then a second of silence, in this room only and at this room's volume. Set the volume
          first: clearly louder than the room's own noise, not uncomfortable.
        </li>
        <li>
          The recording goes to this server to be fitted and is kept nowhere, here or there. Nothing changes in the
          room until you apply what the server proposes.
        </li>
        <li>
          A phone's microphone is not calibrated, least of all in the low bass, and no real phone has been measured
          with this yet: listen to the result, and switch it off or undo it if it is not better.
        </li>
      </ol>
    `}_settings(){let e=this._granted;if(!e)return c;let{settings:t,kept:r,sampleRate:a}=e,o=u=>{let l=t[u];return l===void 0?n`<dd data-setting=${u}>not reported by this browser</dd>`:l===!1?n`<dd data-setting=${u}>off, as asked</dd>`:n`<dd data-setting=${u} data-kept>
        on (${String(l)}): asked off, and the browser kept it on
      </dd>`},i=(u,l)=>u==null?"not reported by this browser":`${u}${l}`;return n`
      <h3 id="granted">What the browser granted</h3>
      <dl aria-labelledby="granted" data-granted>
        ${Tt.map(u=>n`<div class="setting"><dt>${Ms[u]}</dt>${o(u)}</div>`)}
        <div class="setting">
          <dt>Channels</dt>
          <dd data-setting="channelCount">${i(t.channelCount,"")}</dd>
        </div>
        <div class="setting">
          <dt>Microphone's sample rate</dt>
          <dd data-setting="sampleRate">${i(t.sampleRate," Hz")}</dd>
        </div>
        <div class="setting">
          <dt>Recorded at</dt>
          <dd data-setting="recordedAt">${i(a," Hz")}</dd>
        </div>
      </dl>
      ${r.length===0?c:n`<p role="alert" data-flag>
            This browser kept ${r.map(u=>Ms[u].toLowerCase()).join(", ")} on. It changes what
            the microphone hears, so a fit of this recording may be wrong.
          </p>`}
    `}_walk(){let e=this.room,t=this._phase,r=i=>n`
      <div class="row">
        <button type="button" data-step="microphone" aria-label="Use the microphone to measure ${e.name}" @click=${this._onMicrophone}>
          ${i}
        </button>
      </div>
    `;if(t==="guide")return n`${this._guide()} ${r("Use the microphone")}`;if(t==="asking")return n`<p role="status" data-phase="asking">Asking the browser for the microphone.</p>`;if(t==="failed")return n`
        <p role="alert" data-phase="failed">${this._message}</p>
        ${this._settings()} ${r("Measure again")}
      `;if(t==="ready")return n`
        ${this._settings()}
        <p>The microphone is open. The sweep plays as soon as you press, at this room's volume.</p>
        <div class="row">
          <button type="button" data-step="measure" aria-label="Play the sweep in ${e.name} and record" @click=${this._onMeasure}>
            Play the sweep and record
          </button>
          <button type="button" data-step="discard" aria-label="Stop measuring ${e.name}" @click=${this._onDiscard}>Stop</button>
        </div>
      `;if(t==="recording")return n`${this._settings()}
        <p role="status" data-phase="recording">The sweep is playing and the microphone is recording. Hold still.</p>`;if(t==="fitting")return n`${this._settings()}
        <p role="status" data-phase="fitting">The recording is with the server, being fitted. The microphone is off.</p>`;if(t==="refused"){let{name:i,words:u}=this._refused;return n`
        <p role="alert" data-phase="refused" data-refusal=${i||c}>The server refused the recording: ${u}</p>
        <p data-advice>${Xa[i]??Qa}</p>
        ${this._settings()} ${r("Measure again")}
      `}let a=this._fit,o=i=>i===null?"not given":`${i.toFixed(2)} dB`;return n`
      <h3 id="proposed">Proposed filters</h3>
      ${a.filters.length===0?n`<p data-phase="proposed" data-nothing>The server found nothing to correct in this recording.</p>`:n`
            <ul aria-labelledby="proposed" data-phase="proposed">
              ${a.filters.map(i=>n`<li class="figure">${zs(i)}</li>`)}
            </ul>
            <p data-rms>
              The fitter's own figure for this recording, the deviation from flat in the band it fits: ${o(a.rmsBeforeDb)}
              before, ${o(a.rmsAfterDb)} predicted after. It is a prediction from one recording, not a measurement
              of the corrected room.
            </p>
          `}
      ${this._settings()}
      <p>Nothing has changed in the room yet.</p>
      <div class="row">
        ${a.filters.length===0?c:n`<button
              type="button"
              data-step="apply"
              aria-label="Apply the proposed correction to ${e.name}"
              ?disabled=${this._sending}
              @click=${this._onApply}
            >
              Apply
            </button>`}
        <button type="button" data-step="discard" aria-label="Discard the proposed correction for ${e.name}" @click=${this._onDiscard}>
          Discard
        </button>
      </div>
    `}_held(){let e=this.room,{enabled:t,filters:r,undo:a}=e.correction,o=this._busy||this._sending;return n`
      <h3 id="held">This room's correction</h3>
      ${r.length===0?n`<p data-held data-none>This room has no correction.</p>`:n`<ul aria-labelledby="held" data-held>
            ${r.map(i=>n`<li class="figure">${zs(i)}</li>`)}
          </ul>`}
      <div class="row">
        <button
          type="button"
          data-control="enabled"
          aria-label="Correction for ${e.name}"
          aria-pressed=${t===!0?"true":"false"}
          ?disabled=${t===null||r.length===0||o}
          @click=${this._onSwitch}
        >
          Correction
        </button>
        <span data-value="enabled">${t===null?"Unavailable":r.length===0?"Nothing to switch":t?"On":"Off"}</span>
        <button
          type="button"
          data-control="undo"
          aria-label="Undo the last correction of ${e.name}"
          ?disabled=${!a||o}
          @click=${this._onUndo}
        >
          Undo
        </button>
        <span data-value="undo">${a?"Puts back what the room had before the last apply":"Nothing to undo"}</span>
      </div>
      <p role="status" data-done>${this._done}</p>
      <p role="alert" data-command-refusal>${this._commandRefusal?`Refused: ${this._commandRefusal}`:c}</p>
    `}render(){let e=this.room;return e?n`
      <h2>Correction of ${e.name}</h2>
      <h3>Measure this room</h3>
      ${this._walk()} ${this._held()}
    `:n`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`}};customElements.define("chorus-room-correction",Ct);var Rt="room-correction";S({id:Rt,path:"rooms/:room/correction",title:({room:s},e)=>`Correction of ${E(e.rooms,s)?.name??s}`,render:({room:s},{view:e,store:t})=>n`
    <chorus-room-correction
      .room=${E(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .store=${t}
    ></chorus-room-correction>
  `});var Nt=s=>`sound:${s}`,Is=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],to=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],ro=s=>`${s>0?"+":""}${s} dB`,Mt=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    /* A slider keeps a width a finger can travel: in a narrow screen, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(e){return this.renderRoot.querySelector(`input[data-field="${e}"]`)}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of Is){let a=this._slider(r),o=this.room.sound[r];!a||o===null||(!this._held.has(r)||t)&&(a.value=String(o))}}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Nt(this.room.id),body:ne(this.room.id,{[e]:t})},bubbles:!0,composed:!0}))}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onSliderFocus(e){this._held.add(e.target.dataset.field)}_onSliderBlur(e){let t=e.target.dataset.field;this._held.delete(t),this._release(t);let r=this.room?.sound[t];r!=null&&(e.target.value=String(r))}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.field]:Number(e.target.value)}}_onSliderChange(e){let t=e.target.dataset.field;this._release(t),this._ask(t,Number(e.target.value))}_onSwitch(e){let t=e.currentTarget.dataset.field;this._ask(t,!this.room.sound[t])}_tone({field:e,name:t}){let r=this.room,a=r.sound[e],o=a===null?"Unavailable":ro(this._dragged[e]??a);return n`
      <div class="row">
        <label for=${e}>${t}</label>
        ${a===null?c:n`<input
              id=${e}
              data-field=${e}
              type="range"
              min=${K.min}
              max=${K.max}
              step="1"
              aria-label="${t} for ${r.name}"
              aria-valuetext=${o}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${e}>${o}</span>
      </div>
    `}_switch({field:e,name:t,says:r}){let a=this.room,o=a.sound[e];return n`
      <div class="row">
        <button
          type="button"
          data-field=${e}
          aria-label="${t} for ${a.name}"
          aria-pressed=${o===!0?"true":"false"}
          ?disabled=${o===null}
          @click=${this._onSwitch}
        >
          ${t}
        </button>
        <span data-value=${e}>${o===null?"Unavailable":o?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let e=this.room;if(!e)return n`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let t=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c;return n`
      <h2>Sound of ${e.name}</h2>
      ${Is.map(r=>this._tone(r))} ${to.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${t}</p>
    `}};customElements.define("chorus-room-sound",Mt);var zt="room-sound";S({id:zt,path:"rooms/:room/sound",title:({room:s},e)=>`Sound of ${E(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-sound
      .room=${E(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[Nt(s)]??""}
      .refusalField=${r[Nt(s)]??""}
    ></chorus-room-sound>
  `});var It=s=>`theater:${s}`,so=s=>`${s>0?"+":""}${s} ms`,ao=s=>`${s>0?"+":""}${He(s)} dB`,V={av_trim_ms:{name:"A/V trim",range:X,scale:1,step:1,held:s=>s.theater.avTrimMs,words:so,command:(s,e)=>De(s.id,e)},crossover_hz:{name:"Crossover",range:je,scale:1,step:1,held:s=>s.theater.bass.crossoverHz,words:s=>`${s} Hz`,command:(s,e)=>le(s.id,{crossover_hz:e})},sub_level_db:{name:"Sub level",range:Be,scale:100,step:.5,held:s=>s.theater.bass.subLevel,words:ao,command:(s,e)=>le(s.id,{sub_level_db:e})}},oo={off:"Off",ambient:"Ambient"},io={normal:"Normal",inverted:"Inverted"},Lt=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},inputs:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      margin-top: var(--surface-gap);
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    chorus-autoplay {
      margin-top: var(--surface-gap);
    }
    /* A slider keeps a width a finger can travel: in a narrow screen, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label,
    .name {
      min-width: var(--label-min-width);
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    input:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.inputs=[],this.rooms=[],this.groups=[],this.refusals={},this.refusalField="",this._dragged={},this._held=new Set,this._shownRefusal=""}get _refusal(){return this.room&&this.refusals?.[It(this.room.id)]||""}_position(e,t){return String(t/V[e].scale)}updated(){if(!this.room)return;let e=this._refusal,t=!!e&&e!==this._shownRefusal;this._shownRefusal=e,t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-field]")){let a=r.dataset.field,o=V[a].held(this.room);o!==null&&(!this._held.has(a)||t)&&(r.value=this._position(a,o))}}_send(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:It(this.room.id),body:e},bubbles:!0,composed:!0}))}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_read(e){return Math.round(Number(e.value)*V[e.dataset.field].scale)}_onSliderFocus(e){this._held.add(e.target.dataset.field)}_onSliderBlur(e){let t=e.target.dataset.field;this._held.delete(t),this._release(t);let r=this.room?V[t].held(this.room):null;r!==null&&(e.target.value=this._position(t,r))}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.field]:this._read(e.target)}}_onSliderChange(e){let t=e.target.dataset.field;this._release(t),this._send(V[t].command(this.room,this._read(e.target)))}_onNudge(e){let t=this.room.theater.avTrimMs;t!==null&&this._send(De(this.room.id,t+Number(e.currentTarget.dataset.nudge)))}_onUpmix(e){this._send(ne(this.room.id,{tv_upmix:e.currentTarget.dataset.choice}))}_onPolarity(e){this._send(le(this.room.id,{sub_polarity:e.currentTarget.dataset.choice}))}_slider(e){let t=this.room,{name:r,range:a,scale:o,step:i,held:u,words:l}=V[e],p=u(t),m=p===null?"Unavailable":l(this._dragged[e]??p);return n`
      <div class="row">
        <label for=${e}>${r}</label>
        ${p===null?c:n`<input
              id=${e}
              data-field=${e}
              type="range"
              min=${a.min/o}
              max=${a.max/o}
              step=${i}
              aria-label="${r} for ${t.name}"
              aria-valuetext=${m}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${e}>${m}</span>
      </div>
    `}_nudges(){let e=this.room,t=e.theater.avTrimMs,r=(a,o,i)=>n`
      <button
        type="button"
        data-nudge=${a}
        aria-label="A/V trim 1 ms ${o} for ${e.name}"
        ?disabled=${t===null||t+a<X.min||t+a>X.max}
        @click=${this._onNudge}
      >
        ${i}
      </button>
    `;return n`<div class="row">${r(-1,"earlier","1 ms earlier")} ${r(1,"later","1 ms later")}</div>`}_choice({field:e,name:t,words:r,names:a,held:o,onChoose:i}){let u=this.room;return n`
      <div class="row" role="group" aria-label="${t} for ${u.name}">
        <span class="name">${t}</span>
        ${r.map(l=>n`
            <button
              type="button"
              data-choice=${l}
              data-of=${e}
              aria-label="${t} ${a[l].toLowerCase()} for ${u.name}"
              aria-pressed=${o===l?"true":"false"}
              ?disabled=${o===null}
              @click=${i}
            >
              ${a[l]}
            </button>
          `)}
        <span data-value=${e}>${o===null?"Unavailable":a[o]??o}</span>
      </div>
    `}_bass(){let{bass:e}=this.room.theater;return e.active?n`
      ${this._slider("crossover_hz")} ${this._slider("sub_level_db")}
      ${this._choice({field:"sub_polarity",name:"Sub polarity",words:er,names:io,held:e.subPolarity,onChoose:this._onPolarity})}
    `:n`<p data-no-sub>This room's set has no sub, so there is no bass management to set.</p>`}render(){let e=this.room;if(!e)return n`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let t=e.theater;if(!t.offered)return n`
        <h2>Theater of ${e.name}</h2>
        <p role="status" data-none>This room has no TV input and no theater set, so it has no theater settings.</p>
      `;let r=this._refusal,a=r?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${r}`:c;return n`
      <h2>Theater of ${e.name}</h2>
      <h3>A/V trim</h3>
      <p>Later delays this room's TV sound; earlier brings it forward.</p>
      ${this._slider("av_trim_ms")} ${this._nudges()}
      <chorus-autoplay
        tv
        .rows=${this.inputs??[]}
        .rooms=${this.rooms}
        .groups=${this.groups}
        .refusals=${this.refusals}
        .home=${{id:e.id,name:e.name}}
      ></chorus-autoplay>
      <h3>TV upmix</h3>
      <p>
        What the surround speakers of a theater set play from a TV in stereo: nothing, or an ambient
        surround.${t.surrounds?c:" This room's set has no surround speakers now."}
      </p>
      ${this._choice({field:"tv_upmix",name:"TV upmix",words:Zt,names:oo,held:t.tvUpmix,onChoose:this._onUpmix})}
      <h3>Bass management</h3>
      ${this._bass()}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${a}</p>
    `}};customElements.define("chorus-room-theater",Lt);var Pt="room-theater",Ls=s=>s.map(({id:e,name:t})=>({id:e,name:t}));function no(s,e){if(!e)return[];let t=new Map(e.theater.tvInputs.map(({input:r,kind:a})=>[r,a]));return bt(s).filter(r=>t.has(r.input)).map(r=>({...r,kind:t.get(r.input)}))}S({id:Pt,path:"rooms/:room/theater",title:({room:s},e)=>`Theater of ${E(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-theater
      .room=${E(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .inputs=${no(e,E(e.rooms,s))}
      .rooms=${Ls(e.rooms)}
      .groups=${Ls((e.groups??[]).filter(a=>a.kind==="saved"))}
      .refusals=${t}
      .refusalField=${r[It(s)]??""}
    ></chorus-room-theater>
  `});var lo={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},uo=s=>`${Math.round(s/10)}%`,Ut=class extends b{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    .head {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
    }
    .head h2 {
      flex: 1;
    }
    .handle {
      cursor: grab;
      touch-action: none;
      user-select: none;
    }
    select {
      flex: 1;
      min-width: var(--shrink-min);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    h3 {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
      color: var(--muted);
      font-size: var(--meta-size);
    }
    .row {
      display: flex;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
    }
    label {
      min-width: var(--label-min-width);
    }
    /* The slider keeps a width a finger can travel: in a narrow card, or at
     * the kiosk's sizes, the row wraps and the slider takes a line. */
    .row {
      flex-wrap: wrap;
    }
    input[type="range"] {
      flex: 1 1 var(--slider-min-width);
      min-width: var(--slider-min-width);
      height: var(--control-size);
      margin: var(--reset-margin);
      accent-color: var(--accent);
    }
    .figure {
      min-width: var(--figure-min-width);
      font-family: var(--face-figure);
      text-align: end;
    }
    button {
      min-width: var(--control-basis);
      height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    button[aria-pressed="true"] {
      background: var(--control-selected-fill);
      color: var(--control-selected-ink);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    /* A link to a further screen is a control like the buttons beside it. */
    a {
      display: inline-flex;
      box-sizing: border-box;
      align-items: center;
      justify-content: center;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      text-decoration: none;
    }
    a:focus-visible,
    input:focus-visible,
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      margin: var(--reset-margin);
      color: var(--bad);
    }
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._list;t&&(t.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let a=e.has("refusal")&&!!this.refusal;a&&(this._dragged=null),(!this._sliderHeld||a)&&(r.value=String(this.room.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(Kt(this.room.id,Number(e.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(e){let t=e.target.value;if(e.target.value=this.place,t===this.place)return;let r=ls(t);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(Yt(this.room.id,!this.room.muted))}render(){let e=this.room;if(!e)return c;let t=e.volume===null?"Unavailable":uo(this._dragged??e.volume);return n`
      <div class="head">
        <h2>${e.name}</h2>
        <button
          type="button"
          class="handle"
          data-drag-room=${e.id}
          aria-label="Move ${e.name}"
          title="Drag onto a room or a group, or press to choose from the list"
          @click=${this._onHandle}
        >
          Move
        </button>
        <a href=${A(zt,{room:e.id})} data-route aria-label="Sound for ${e.name}">Sound</a>
        <a href=${A(xt,{room:e.id})} data-route aria-label="Limits for ${e.name}">Limits</a>
        <a href=${A(Rt,{room:e.id})} data-route aria-label="Correction for ${e.name}">Correction</a>
        ${e.theater?.offered?n`<a href=${A(Pt,{room:e.id})} data-route aria-label="Theater for ${e.name}">Theater</a>`:c}
      </div>
      ${e.bond.length===0?c:n`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${e.bond.map(r=>n`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${lo[r.role]??r.role}: ${r.name}
                  </li>`)}
            </ul>
          `}
      ${e.source?n`<chorus-playing
            .target=${e.id}
            .name=${e.name}
            .source=${e.source}
            .nowPlaying=${e.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:c}
      <div class="row">
        <label for="volume">Volume</label>
        ${e.volume===null?c:n`<input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Volume for ${e.name}"
              aria-valuetext=${t}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-volume>${t}</span>
      </div>
      <div class="row">
        <button
          type="button"
          aria-label="Mute ${e.name}"
          aria-pressed=${e.muted===!0?"true":"false"}
          ?disabled=${e.muted===null}
          @click=${this._onMute}
        >
          Mute
        </button>
        <span data-mute>${e.muted===null?"Unavailable":e.muted?"Muted":"Not muted"}</span>
      </div>
      <div class="row">
        <label for="place">Plays with</label>
        <select id="place" aria-label="Group for ${e.name}" @change=${this._onPlace}>
          ${this.places.map(r=>n`<option value=${r.value} ?selected=${r.value===this.place}>${r.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",Ut);var Ft=class extends b{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
    :host {
      display: block;
    }
    ul {
      display: flex;
      flex-direction: column;
      gap: var(--surface-gap);
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    p[data-status="lost"],
    p[data-status="signed-out"] {
      color: var(--warn);
    }
    li[data-moving] {
      border-radius: var(--surface-radius);
      outline: var(--stroke-1) dashed var(--border);
      outline-offset: var(--focus-ring-offset);
    }
    li[data-over] {
      border-radius: var(--surface-radius);
      outline: var(--focus-ring-width) solid var(--accent);
      outline-offset: var(--focus-ring-offset);
    }
    code {
      font-family: var(--face-figure);
      font-size: var(--code-size);
    }
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let e=this.rooms,t=this.groups??[],r=this.over;return n`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${e!==null&&e.length===0?n`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${Ce(e??[],a=>a.id,a=>n`<li
              data-room=${a.id}
              data-drop="room"
              data-drop-id=${a.id}
              ?data-moving=${this.moving?.id===a.id}
              ?data-over=${r?.kind==="room"&&r.id===a.id&&this.moving?.id!==a.id}
            >
              <chorus-room-card
                .room=${a}
                .inputs=${this.inputs}
                .refusal=${this.refusals[a.id]??""}
                .places=${us(a,e,t)}
                .place=${ds(a,t)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",Ft);var Dt="chorus-setup-",jt=6,co=12,ho=Object.freeze({form:"GET /",takes:"POST /join",title:"chorus speaker setup"}),po=Object.freeze([{id:"power",title:"Switch the speaker on",text:[`A Wi-Fi speaker that knows no network raises a Wi-Fi access point of its own, named ${Dt} and ${jt} characters (${Dt}<${jt} characters>).`,`Its setup secret is ${co} characters and is the access point's password. The speaker prints it, and the address of its join page, on its serial console when the access point comes up.`]},{id:"access-point",title:"Join the speaker's access point",text:[`In this phone's Wi-Fi settings, join the network ${Dt}<${jt} characters> with the setup secret as its password. Accept that it has no internet.`,"The phone is then off the house's network, and this page cannot reach the chorus server until it is back. That is expected. Leave this page open."]},{id:"join-page",title:"Open the speaker's join page",text:[`In the phone's browser, open the address the speaker printed (http://<address>/). The speaker serves the page itself, on its access point: it is titled "${ho.title}" and is a form with two fields.`,"Type the house network's name and its passphrase into that page, and press Join. They go to the speaker and nowhere else: this app never asks for them. The network has to be on 2.4 GHz and have a passphrase; the speaker refuses an open network.",'The page answers "Received". If the join fails the access point stays up: join it again and load the page again, and it says why above the form (auth-error for a wrong passphrase, network-not-found for a name it cannot see).']},{id:"return",title:"Come back to the house's network",text:["The speaker takes its access point down and joins the house's network. The phone goes back to the house's network on its own, or join it again in the Wi-Fi settings.","When the speaker reaches the chorus server it is adopted, and this page says so by itself. There is nothing to press."]}]);function fo(s,e){let t=new Set(s??[]);return(e??[]).filter(r=>!t.has(r.id))}var Ht="chorus-speaker-setup";function mo(s){try{let e=JSON.parse(s?.getItem(Ht)??"null");return Array.isArray(e)&&e.every(t=>typeof t=="string")?e:null}catch{return null}}function Bt(s,e){try{e===null?s?.removeItem(Ht):s?.setItem(Ht,JSON.stringify(e))}catch{}}var go=()=>{try{return globalThis.sessionStorage??null}catch{return null}},Wt=class extends b{static properties={speakers:{attribute:!1},status:{type:String},back:{type:String},storage:{attribute:!1},_baseline:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3 {
      font-size: var(--body-size);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ol {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li,
    [data-done],
    [role="status"] {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li p + p {
      margin-top: var(--control-pad-y);
    }
    [role="status"] {
      font-size: var(--body-size);
    }
    [data-away] {
      color: var(--warn);
    }
    [data-done] p:first-of-type {
      color: var(--ok);
      font-size: var(--body-size);
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      margin-top: var(--surface-gap);
    }
    button,
    a {
      display: inline-flex;
      box-sizing: border-box;
      align-items: center;
      justify-content: center;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
      text-decoration: none;
    }
    button:focus-visible,
    a:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
  `;constructor(){super(),this.speakers=null,this.status="connecting",this.back="#/",this.storage=go(),this._baseline=null}willUpdate(){!this.isConnected||this._baseline!==null||!Array.isArray(this.speakers)||(this._baseline=mo(this.storage)??this.speakers.map(e=>e.id),Bt(this.storage,this._baseline))}disconnectedCallback(){super.disconnectedCallback(),Bt(this.storage,null),this._baseline=null}_onAgain(){Array.isArray(this.speakers)&&(this._baseline=this.speakers.map(e=>e.id),Bt(this.storage,this._baseline))}_status(e){return e.length>0?c:this.status==="lost"||this.status==="signed-out"?n`<p role="status" data-away>
        ${this.status==="signed-out"?"Signed out of the chorus server: sign in again to go on.":"This page cannot reach the chorus server now. That is expected while the phone is on the speaker's access point: it goes on by itself when the phone is back on the house's network."}
      </p>`:this._baseline===null?n`<p role="status" data-waiting>Reading this server's speakers.</p>`:n`<p role="status" data-waiting>
      Waiting for a new speaker. This page goes on by itself when one is adopted.
    </p>`}_done(e){return e.length===0?c:n`
      <div data-done role="status">
        ${e.map(t=>n`<p data-arrived=${t.id}>${t.name} (${t.id}) joined and was adopted.</p>`)}
        <p>It has no name of its own and is in no room yet.</p>
        <div class="row">
          <a href=${this.back} data-route aria-label="Name the new speaker and give it a room">Name it and give it a room</a>
          <button type="button" aria-label="Set up another speaker" @click=${this._onAgain}>Set up another</button>
        </div>
      </div>
    `}render(){let e=this._baseline===null?[]:fo(this._baseline,this.speakers);return n`
      <h2>Set up a Wi-Fi speaker</h2>
      <p>
        A compact Wi-Fi speaker learns the house's network from a phone, on a page the speaker serves itself. This app
        says the steps and watches for the speaker; it never asks for the network's passphrase.
      </p>
      ${this._done(e)}
      <ol aria-label="Steps" ?data-complete=${e.length>0}>
        ${po.map((t,r)=>n`
            <li data-step=${t.id}>
              <h3>${r+1}. ${t.title}</h3>
              ${t.text.map(a=>n`<p>${a}</p>`)}
            </li>
          `)}
      </ol>
      ${this._status(e)}
    `}};customElements.define("chorus-speaker-setup",Wt);var Ps=s=>`speaker:${s}`,Us="firmware",vo={wired:"Wired",wireless:"Wi-Fi"},Fs=["requested","receiving","verified","pending_verify"],bo=["requested","receiving"];function _o(s){let e=s.imageVersion?`version ${s.imageVersion}`:"";return s.image?e?`image ${s.image} (${e})`:`image ${s.image}`:e||"an image"}function $o(s){let e=_o(s),t=s.version?`version ${s.version}`:"the version it ran before";switch(s.state){case"idle":return"No install is in progress.";case"requested":return`Install requested: the server is offering ${e} to the speaker.`;case"receiving":return`Receiving ${e}: ${s.received} of ${s.size} bytes.`;case"verified":return`Written and checked: ${e}. The speaker restarts into it.`;case"pending_verify":return`On trial: the speaker runs ${t} and has not confirmed it yet.`;case"confirmed":return`Installed: ${e} confirmed itself, and the speaker runs ${t}.`;case"rolled_back":return`Rolled back: ${e} did not confirm, and the speaker runs ${t} again. Nothing retries it.`;case"refused":return`Refused by the speaker: ${e} was not installed.`;case"interrupted":return`Interrupted: the install of ${e} did not finish and is not resumed. Install again to start over.`;case"cancelled":return`Cancelled: the install of ${e} was abandoned.`;default:return`Firmware state: ${s.state}.`}}var qt=class extends b{static properties={speakers:{attribute:!1},keyChanges:{attribute:!1},rooms:{attribute:!1},images:{attribute:!1},refusals:{attribute:!1},setup:{type:String},_drafts:{state:!0},_forgetting:{state:!0},_installing:{state:!0}};static styles=$`
    :host {
      display: block;
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
      background: var(--panel);
    }
    h2,
    h3,
    h4 {
      margin: var(--reset-margin);
      font-size: var(--heading-size);
    }
    h3,
    h4 {
      font-size: var(--body-size);
    }
    section {
      margin-top: var(--surface-gap);
    }
    [data-update-available] {
      color: var(--warn);
      font-size: var(--body-size);
    }
    progress {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    ul {
      margin: var(--reset-margin);
      padding: var(--reset-margin);
      list-style: none;
    }
    li {
      margin-top: var(--surface-gap);
      padding: var(--surface-pad);
      border: var(--stroke-1) solid var(--border);
      border-radius: var(--surface-radius);
    }
    li[data-new],
    li[data-key-change] {
      border-color: var(--warn);
    }
    [data-new-mark],
    [data-key-changed] {
      color: var(--warn);
      font-size: var(--body-size);
    }
    dl {
      display: flex;
      flex-wrap: wrap;
      gap: var(--control-pad-y) var(--surface-gap);
      margin: var(--reset-margin);
      font-size: var(--meta-size);
    }
    dl div {
      display: flex;
      gap: var(--control-pad-y);
    }
    dt {
      color: var(--muted);
    }
    dd {
      margin: var(--reset-margin);
      overflow-wrap: anywhere;
    }
    code,
    [data-id] {
      font-family: var(--font-mono);
      overflow-wrap: anywhere;
    }
    .row {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      min-height: var(--control-size);
      margin-top: var(--control-pad-y);
    }
    label {
      min-width: var(--label-min-width);
    }
    button,
    select,
    input,
    a {
      box-sizing: border-box;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    a {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      margin-top: var(--surface-gap);
      text-decoration: none;
    }
    select,
    input {
      flex: 1 1 var(--control-basis);
      min-width: var(--shrink-min);
    }
    button:disabled {
      color: var(--control-disabled-ink);
    }
    button[data-forget="yes"] {
      border-color: var(--bad);
      color: var(--bad);
    }
    select:focus-visible,
    input:focus-visible,
    a:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      color: var(--bad);
      font-size: var(--body-size);
    }
  `;constructor(){super(),this.speakers=null,this.keyChanges=[],this.rooms=[],this.refusals={},this.setup="",this.images=null,this._drafts={},this._forgetting=null,this._installing=null}_speaker(e){return(this.speakers??[]).find(t=>t.id===e)??null}willUpdate(e){if(this._installing!==null&&(e.has("speakers")||e.has("images"))){let r=this._speaker(this._installing.speaker);r&&this._offers(r).some(o=>o.name===this._installing.image)||(this._installing=null)}if(!e.has("speakers"))return;let t=Object.entries(this._drafts).filter(([r,a])=>{let o=this._speaker(r);return o&&!(o.named&&o.name===a.trim())});t.length!==Object.keys(this._drafts).length&&(this._drafts=Object.fromEntries(t)),this._forgetting!==null&&!this._speaker(this._forgetting)&&(this._forgetting=null)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Ps(e),body:t},bubbles:!0,composed:!0}))}_onDraft(e){this._drafts={...this._drafts,[e.target.dataset.speaker]:e.target.value}}_name(e){let t=this._speaker(e),r=(this._drafts[e]??t?.name??"").trim();!t||!r||t.named&&r===t.name||this._ask(e,dr(e,r))}_onName(e){this._name(e.currentTarget.dataset.speaker)}_onNameKey(e){e.key==="Enter"&&(e.preventDefault(),this._name(e.target.dataset.speaker))}_onRoom(e){let t=this._speaker(e.target.dataset.speaker),r=e.target.value,a=t?.room??"";e.target.value=a,!(!t||r===a)&&this._ask(t.id,ur(t.id,r||null))}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-speaker]")){let t=this._speaker(e.dataset.speaker)?.room??"";e.value!==t&&(e.value=t)}}_onForget(e){let{speaker:t,forget:r}=e.currentTarget.dataset;if(r==="ask"){this._forgetting=t;return}this._forgetting=null,r==="yes"&&this._ask(t,cr(t))}_offers(e){let t=e.firmware;return!t||this.images===null||Fs.includes(t.state)?[]:Xr(t,this.images)}_onInstall(e){let{speaker:t,image:r,install:a}=e.currentTarget.dataset;if(a==="ask"){this._installing={speaker:t,image:r};return}let o=this._installing;this._installing=null,!(a!=="yes"||!o||o.speaker!==t||o.image!==r)&&this._ask(t,hr(t,r))}_onCancelInstall(e){let{speaker:t}=e.currentTarget.dataset;this._ask(t,pr(t))}_onRescan(){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Us,body:fr()},bubbles:!0,composed:!0}))}_rooms(e){let t=this.rooms??[],r=e.room===null||t.some(a=>a.id===e.room);return n`
      <option value="" ?selected=${e.room===null}>No room</option>
      ${r?c:n`<option value=${e.room} selected>${e.room} (not on this server now)</option>`}
      ${t.map(a=>n`<option value=${a.id} ?selected=${e.room===a.id}>${a.name}</option>`)}
    `}_forget(e){let{id:t,name:r}=e;return this._forgetting!==t?n`
        <div class="row">
          <button type="button" data-speaker=${t} data-forget="ask" aria-label="Forget ${r}" @click=${this._onForget}>
            Forget
          </button>
        </div>
      `:n`
      <p data-forget-question>
        Forget ${r}? Its name, its room and its pinned key are removed. If it connects again it is adopted as a new
        speaker, whatever key it then offers.
      </p>
      <div class="row">
        <button type="button" data-speaker=${t} data-forget="yes" aria-label="Yes, forget ${r}" @click=${this._onForget}>
          Yes, forget it
        </button>
        <button type="button" data-speaker=${t} data-forget="no" aria-label="Keep ${r}" @click=${this._onForget}>
          Keep it
        </button>
      </div>
    `}_offer(e,t){let{id:r,name:a}=e,o=t.version??"not said";return this._installing?.speaker===r&&this._installing?.image===t.name?n`
      <div data-update=${t.name}>
        <p data-install-question>
          Install image ${t.name} (version ${o}) on ${a} (${r})? It runs version
          ${e.firmware.version??"not said"} now. The image is sent to the speaker, which restarts into it and
          stops playing while it does. If the new version does not confirm itself, the speaker goes back to the one it
          runs now.
        </p>
        <div class="row">
          <button
            type="button"
            data-speaker=${r}
            data-image=${t.name}
            data-install="yes"
            aria-label="Yes, install image ${t.name} on ${a}"
            @click=${this._onInstall}
          >
            Yes, install it
          </button>
          <button
            type="button"
            data-speaker=${r}
            data-image=${t.name}
            data-install="no"
            aria-label="Do not install image ${t.name} on ${a}"
            @click=${this._onInstall}
          >
            Not now
          </button>
        </div>
      </div>
    `:n`
        <div class="row" data-update=${t.name}>
          <span data-update-version>Version ${o}, image ${t.name}</span>
          <button
            type="button"
            data-speaker=${r}
            data-image=${t.name}
            data-install="ask"
            aria-label="Install image ${t.name} (version ${o}) on ${a}"
            ?disabled=${!e.present}
            @click=${this._onInstall}
          >
            Install
          </button>
        </div>
      `}_firmware(e){let t=e.firmware;if(!t)return c;let{id:r,name:a}=e,o=this.images!==null,i=this._offers(e),u=t.state==="receiving"&&t.size>0;return n`
      <section data-firmware aria-label="Firmware of ${a}">
        <h4>Firmware</h4>
        <dl>
          <div>
            <dt>Runs</dt>
            <dd data-value="firmware-version">${t.version??"Not said"}</dd>
          </div>
          <div>
            <dt>Board</dt>
            <dd data-value="firmware-board">${t.board??"Not said"}</dd>
          </div>
          <div>
            <dt>Slot</dt>
            <dd data-value="firmware-slot">${t.slot??"Not said"}</dd>
          </div>
        </dl>
        <p role="status" data-firmware-state=${t.state}>
          ${$o(t)}
          ${t.reason?n`<span data-firmware-reason>Reason: ${t.reason}.</span>`:c}
        </p>
        ${u?n`<div class="row">
              <progress
                max=${t.size}
                value=${Math.min(t.received,t.size)}
                aria-label="Install progress of ${a}"
              ></progress>
            </div>`:c}
        ${o&&bo.includes(t.state)?n`<div class="row">
              <button
                type="button"
                data-speaker=${r}
                data-cancel-install
                aria-label="Cancel the install on ${a}"
                @click=${this._onCancelInstall}
              >
                Cancel install
              </button>
            </div>`:c}
        ${o&&t.updateAvailable?n`
              <p data-update-available>Update available</p>
              ${i.map(l=>this._offer(e,l))}
              ${i.length>0&&!e.present?n`<p data-update-absent>The speaker is not connected: it can be installed when it is.</p>`:c}
              ${i.length===0&&!Fs.includes(t.state)?n`<p data-update-unlisted>
                    The server lists no verified image for this board with another version. Rescan the staged images.
                  </p>`:c}
            `:c}
      </section>
    `}_images(){if(this.images===null)return c;let e=this.refusals?.[Us]??"";return n`
      <section data-firmware-images aria-label="Firmware images">
        <h3>Firmware images</h3>
        <p>
          Images are staged as files in the server's firmware directory. Nothing is installed until you press Install
          on a speaker and confirm it.
        </p>
        ${this.images.length===0?n`<p role="status" data-no-images>No image is staged.</p>`:n`<ul aria-label="Staged images">
              ${this.images.map(t=>n`
                  <li data-image=${t.name}>
                    <h4>${t.name}</h4>
                    <dl>
                      <div>
                        <dt>Version</dt>
                        <dd data-value="version">${t.version??"Not read"}</dd>
                      </div>
                      <div>
                        <dt>Board</dt>
                        <dd data-value="board">${t.board??"Not read"}</dd>
                      </div>
                      <div>
                        <dt>Verdict</dt>
                        <dd data-value="verdict">
                          ${t.verified?"Verified":`Refused: ${t.reason??"no reason given"}. It is never offered to a speaker.`}
                        </dd>
                      </div>
                    </dl>
                  </li>
                `)}
            </ul>`}
        <div class="row">
          <button type="button" data-rescan aria-label="Rescan the staged firmware images" @click=${this._onRescan}>
            Rescan
          </button>
        </div>
        <p role="alert">${e?`Refused: ${e}`:c}</p>
      </section>
    `}_row(e){let{id:t,name:r}=e,a=this.refusals?.[Ps(t)]??"",o=(this.keyChanges??[]).some(l=>l.id===t),i=this._drafts[t],u=(i??r).trim();return n`
      <li data-speaker=${t} ?data-new=${e.isNew}>
        <h3>${r}</h3>
        ${e.isNew?n`<p data-new-mark>New: adopted, not named and in no room yet.</p>`:c}
        ${o?n`<p data-key-changed>A session under this id offered another key and was refused (above).</p>`:c}
        <p data-id>${t}</p>
        <dl>
          <div>
            <dt>Now</dt>
            <dd data-value="present">${e.present?"Connected":"Not connected"}</dd>
          </div>
          <div>
            <dt>Link</dt>
            <dd data-value="link">${vo[e.link]??"Not reported"}</dd>
          </div>
          <div>
            <dt>Software</dt>
            <dd data-value="software">${e.software??"Not said yet"}</dd>
          </div>
          <div>
            <dt>Key</dt>
            <dd data-value="key">${e.key??"Not known"}</dd>
          </div>
        </dl>
        <div class="row">
          <label for="name-${t}">Name</label>
          <input
            id="name-${t}"
            type="text"
            autocomplete="off"
            data-speaker=${t}
            aria-label="Name of ${r}"
            .value=${i??r}
            @input=${this._onDraft}
            @keydown=${this._onNameKey}
          />
          <button
            type="button"
            data-speaker=${t}
            aria-label="Save the name of ${r}"
            ?disabled=${!u||e.named&&u===r}
            @click=${this._onName}
          >
            Save name
          </button>
        </div>
        <div class="row">
          <label for="room-${t}">Room</label>
          <select id="room-${t}" data-speaker=${t} aria-label="Room of ${r}" @change=${this._onRoom}>
            ${this._rooms(e)}
          </select>
        </div>
        ${this._firmware(e)}
        ${this._forget(e)}
        <p role="alert">${a?`Refused: ${a}`:c}</p>
      </li>
    `}_keyChange(e){let t=this._speaker(e.id);return n`
      <li data-key-change=${e.id}>
        <h3>Refused: ${t?.name??e.id} offered a changed key</h3>
        <p role="alert">
          A session under the id <span data-id>${e.id}</span> offered a key that is not the one this id is pinned
          to. The server refused it, and the pinned key did not move.
        </p>
        <dl>
          <div>
            <dt>Pinned key</dt>
            <dd><code data-value="pinned">${e.pinned??"not known"}</code></dd>
          </div>
          <div>
            <dt>Offered key, refused</dt>
            <dd><code data-value="offered">${e.offered??"not known"}</code></dd>
          </div>
        </dl>
        <p>
          Nothing here accepts the offered key.
          ${t?`If you replaced or wiped this speaker yourself, forget ${t.name} below: its next session is then adopted as a new speaker. If you did not, something else is answering under its id.`:"This id is not among the speakers listed here, so nothing on this screen can forget it."}
        </p>
      </li>
    `}render(){if(this.speakers===null)return n`<p role="status" data-missing>Reading this server's speakers.</p>`;let e=this.keyChanges??[];return n`
      <h2>Speakers</h2>
      <p>A speaker is adopted when it first connects. Name it and put it in a room here.</p>
      ${e.length===0?c:n`<ul aria-label="Changed keys">
            ${e.map(t=>this._keyChange(t))}
          </ul>`}
      ${this.speakers.length===0?n`<p role="status" data-none>This server has adopted no speaker yet.</p>`:n`<ul aria-label="Adopted speakers">
            ${this.speakers.map(t=>this._row(t))}
          </ul>`}
      ${this._images()}
      ${this.setup?n`<a href=${this.setup} data-route aria-label="Set up a Wi-Fi speaker">Set up a Wi-Fi speaker</a>`:c}
    `}};customElements.define("chorus-speakers",qt);var Me="speakers",Ds="speaker-setup";S({id:Me,path:"speakers",title:()=>"Speakers",render:(s,{view:e,refusals:t})=>n`
    <chorus-speakers
      .speakers=${e.state===null?null:ut(e.state)}
      .keyChanges=${Qr(e.state)}
      .rooms=${e.rooms.map(({id:r,name:a})=>({id:r,name:a}))}
      .images=${Yr(e.state)}
      .refusals=${t}
      .setup=${A(Ds)}
    ></chorus-speakers>
  `});S({id:Ds,path:"speakers/setup",title:()=>"Set up a Wi-Fi speaker",render:(s,{view:e})=>n`
    <chorus-speaker-setup
      .speakers=${e.state===null?null:ut(e.state)}
      .status=${e.status}
      .back=${A(Me)}
    ></chorus-speaker-setup>
  `});var Vt=class extends b{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
    :host {
      display: block;
    }
    header {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: var(--surface-gap);
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
    }
    nav {
      display: flex;
      box-sizing: border-box;
      gap: var(--bar-gap-x);
    }
    nav button {
      box-sizing: border-box;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      font: inherit;
    }
    :focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    /* A region the navigation went to holds the focus without a ring of its
     * own: what is ringed is the control a person then moves to. */
    section:focus,
    main:focus {
      outline: none;
    }
    section,
    main {
      min-width: var(--shrink-min);
    }

    /* The phone: one column, and the navigation fixed to the bottom edge,
     * under the thumb. The host keeps the bar's height free below the last
     * card, so the bar covers nothing. */
    :host([layout="phone"]) {
      padding-bottom: calc(var(--control-size) + var(--bar-pad-y) + var(--bar-pad-y) + var(--hairline));
    }
    :host([layout="phone"]) nav {
      position: fixed;
      right: var(--reset-margin);
      bottom: var(--reset-margin);
      left: var(--reset-margin);
      z-index: 1;
      padding: var(--bar-pad-y) var(--bar-pad-x);
      border-top: var(--hairline) solid var(--border);
      background: var(--panel);
    }
    :host([layout="phone"]) nav button {
      flex: 1 1 var(--control-basis);
    }

    /* The desktop: the groups, with what each plays, beside the rooms; the
     * header and the signed-out words run across both columns. */
    :host([layout="desktop"]) {
      display: grid;
      grid-template-columns: minmax(var(--surface-column-min), 1fr) minmax(var(--surface-column-min), 2fr);
      align-items: start;
    }
    :host([layout="desktop"]) header,
    :host([layout="desktop"]) [data-signed-out] {
      grid-column: 1 / -1;
    }
    :host([layout="desktop"]) nav {
      margin-left: auto;
    }
    :host([layout="desktop"]) section {
      padding-bottom: var(--surface-pad);
    }
    h1 {
      margin: var(--reset-margin);
      color: var(--wordmark-ink);
      font-size: var(--wordmark-size);
      letter-spacing: var(--tracking-wordmark);
    }
    section,
    main {
      padding: var(--surface-pad);
    }
    /* A further screen: alone on the page, across both columns, under the
     * way back. The link is a control like the navigation's buttons. */
    :host([layout="desktop"]) main[data-screen] {
      grid-column: 1 / -1;
    }
    main[data-screen] {
      display: flex;
      flex-direction: column;
      align-items: stretch;
      gap: var(--surface-gap);
    }
    a[data-route] {
      display: inline-flex;
      box-sizing: border-box;
      align-items: center;
      justify-content: center;
      align-self: start;
      min-width: var(--control-basis);
      min-height: var(--control-size);
      padding: var(--control-pad-y) var(--control-pad-x);
      border: var(--stroke-1) solid var(--control-edge);
      border-radius: var(--control-radius);
      background: var(--control-surface);
      color: var(--control-ink);
      text-decoration: none;
    }
    section {
      padding-bottom: var(--reset-margin);
    }
    /* The home's links to the house-wide screens, under the rooms. */
    a.more {
      margin-top: var(--surface-gap);
    }
    a.more + a.more {
      margin-left: var(--surface-gap);
    }
    p {
      margin: var(--reset-margin);
      color: var(--muted);
      font-size: var(--meta-size);
    }
    [data-signed-out] {
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
      color: var(--warn);
      font-size: var(--body-size);
    }
    [data-signed-out] a {
      color: var(--link);
    }
    /* A wall tablet: every control at the kiosk's least size, larger text,
     * and nothing of the app around the rooms. The sizes are tokens the
     * elements under this one already read, set anew for them here. Wide, it
     * shows both columns and has nothing to navigate between; narrow, it
     * keeps the bar at the bottom edge. */
    :host([mode="kiosk"]) {
      --control-size: var(--kiosk-control-size);
      --control-basis: var(--kiosk-control-size);
      --control-basis-narrow: var(--kiosk-control-size);
      --body-size: var(--kiosk-body-size);
      --meta-size: var(--kiosk-meta-size);
      --heading-size: var(--kiosk-heading-size);
      font-size: var(--body-size);
    }
    :host([mode="kiosk"]) h1 {
      display: none;
    }
    :host([mode="kiosk"][layout="phone"]) header {
      padding: var(--reset-margin);
      border-bottom-width: var(--reset-margin);
    }
    :host([mode="kiosk"][layout="desktop"]) header {
      display: none;
    }
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=Br(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",e=>this._onLink(e)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=Fr({onStart:e=>{let t=this._room(e);t&&(this._moving={id:e,name:t.name,grouped:!!ae(t,this._groups)})},onOver:e=>{let t=this._over;t?.kind===e?.kind&&t?.id===e?.id||(this._over=e)},onEnd:(e,t)=>{this._moving=null,this._over=null,t&&this._move(e,t)}})}get _groups(){return this._view.groups??[]}_room(e){return this._view.rooms.find(t=>t.id===e)??null}willUpdate(e){Ss.includes(this.mode)||(this.mode="app"),_s.includes(this.layout)||(this.layout="phone"),e.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=$s(e=>{this.layout=e}),this._unroute?.(),this._unroute=this._navigation.watch(e=>{e.address!==this._route.address&&(this._route=e)})}updated(e){if(!e.has("_route")||e.get("_route")===void 0)return;let t=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(t==="groups"?"section":"main");r&&(t&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!t}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(e=>{this._view=e}))}async _send(e,t){if(!this.store)return;this._refusals={...this._refusals,[e]:""},this._refusalFields={...this._refusalFields,[e]:""};let r=await this.store.command(t);r.ok||(this._refusals={...this._refusals,[e]:r.refusal},this._refusalFields={...this._refusalFields,[e]:r.field??""})}_onCommand(e){let{subject:t,room:r,body:a,done:o}=e.detail;this._send(t??r,a).then(()=>o?.())}_move(e,t){let r=this._room(e),a=ns(r,t,this._groups);a&&this._send(e,a)}_onMove(e){this._move(e.detail.room,e.detail.destination)}_onPointerDown(e){this._drag.begin(e)}_onGo(e){let t=e.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=t,this._navigation.back();return}let r=this.renderRoot.querySelector(t==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(e){if(e.defaultPrevented||e.button>0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;let t=e.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));t&&(e.preventDefault(),t.dataset.route==="back"?this._navigation.back():this._navigation.open(t.getAttribute("href")))}_screen(e){let t=at(e.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields,store:this.store};return n`
      <main
        aria-label=${t.title(e.params,this._view)}
        data-screen=${t.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${Se} data-route="back" aria-label="Back to rooms">Back</a>
        ${t.render(e.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?c:n`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return n`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()} ${this._route.screen===ke.screen?this._home():this._screen(this._route)}
    `}_home(){return n`
      <section
        aria-label="Groups"
        tabindex="-1"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
      >
        <chorus-groups
          .groups=${this._view.state===null?null:this._groups}
          .inputs=${this._view.inputs??[]}
          .refusals=${this._refusals}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-groups>
        <p role="status" data-drag>
          ${this._moving?`Moving ${this._moving.name}. Drop it on a room or a group.`:c}
        </p>
      </section>
      <main
        aria-label="Rooms"
        tabindex="-1"
        @chorus-command=${this._onCommand}
        @chorus-move=${this._onMove}
        @pointerdown=${this._onPointerDown}
      >
        <chorus-rooms
          .rooms=${this._view.state===null?null:this._view.rooms}
          .status=${this._view.status}
          .inputs=${this._view.inputs??[]}
          .refusals=${this._refusals}
          .groups=${this._groups}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-rooms>
        <a class="more" href=${A(_t)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${A(mt)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <a class="more" href=${A(Me)} data-route aria-label="Speakers and their setup">Speakers</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",Vt);var yo="sw.js";async function js(s=globalThis.navigator){let e=s?.serviceWorker;if(!e||typeof e.register!="function")return null;try{return await e.register(yo,{scope:"./",updateViaCache:"none"})}catch{return null}}function Bs({navigator:s=globalThis.navigator,document:e=globalThis.document}={}){let t=null;try{t=s?.wakeLock??null}catch{t=null}if(!t||typeof t.request!="function"||typeof e?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,a=null,o=!1,i=async l=>{try{await l.release()}catch{}},u=()=>{o||r||a||e.visibilityState!=="visible"||(a=(async()=>{try{let l=await t.request("screen");if(o){await i(l);return}r=l,l.addEventListener?.("release",()=>{r===l&&(r=null)})}catch{}finally{a=null}})())};return e.addEventListener("visibilitychange",u),u(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;a;)await a},stop:async()=>{for(o=!0,e.removeEventListener("visibilitychange",u);a;)await a;let l=r;r=null,l&&await i(l)}}}var ze=document.querySelector("chorus-app");if(ze){ze.mode=xs(window.location.search,As(window)),ze.mode==="kiosk"&&Bs();let s=es(br());ze.store=s,s.start()}js();
/*! Bundled license information:

@lit/reactive-element/css-tag.js:
  (**
   * @license
   * Copyright 2019 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

@lit/reactive-element/reactive-element.js:
lit-html/lit-html.js:
lit-element/lit-element.js:
lit-html/directive.js:
lit-html/directives/repeat.js:
  (**
   * @license
   * Copyright 2017 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

lit-html/is-server.js:
  (**
   * @license
   * Copyright 2022 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

lit-html/directive-helpers.js:
  (**
   * @license
   * Copyright 2020 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)

lit-html/directives/keyed.js:
  (**
   * @license
   * Copyright 2021 Google LLC
   * SPDX-License-Identifier: BSD-3-Clause
   *)
*/
