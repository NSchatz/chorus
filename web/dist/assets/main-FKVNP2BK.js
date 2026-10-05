function q(s){let e=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(e/1e3)}.${String(e%1e3).padStart(3,"0")}`}function Mt(s,e){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${q(e)}}`}function Lt(s,e){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${e?"true":"false"}}`}function Ne(s,e){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(e)}}`}function re(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function Pt(s,e){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(e)}}`}function Ut(s,e){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${q(e)}}`}var J=Object.freeze({min:-10,max:10}),_s=["bass","treble"],ys=["loudness","night","speech"];function se(s,e={}){let t=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of _s){if(e[r]===void 0)continue;let a=Math.min(J.max,Math.max(J.min,Math.round(Number(e[r])||0)));t+=`,"${r}":${a}`}for(let r of ys)e[r]!==void 0&&(t+=`,"${r}":${e[r]?"true":"false"}`);return e.tv_upmix!==void 0&&(t+=`,"tv_upmix":${JSON.stringify(String(e.tv_upmix))}`),`${t}}`}var Ft=Object.freeze(["off","ambient"]),G=Object.freeze({min:-100,max:200}),Re=(s,{min:e,max:t})=>Math.min(t,Math.max(e,Math.round(Number(s)||0)));function ze(s,e){return`{"v":2,"t":"av_trim","zone":${JSON.stringify(s)},"av_trim_ms":${Re(e,G)}}`}var Ie=Object.freeze({min:40,max:200}),Me=Object.freeze({min:-1200,max:600}),Dt=Object.freeze(["normal","inverted"]);function Le(s){let e=Re(s,Me),t=Math.abs(e);return`${e<0?"-":""}${Math.floor(t/100)}.${String(t%100).padStart(2,"0")}`}function ae(s,e={}){let t=`{"v":2,"t":"bass_management","zone":${JSON.stringify(s)}`;return e.crossover_hz!==void 0&&(t+=`,"crossover_hz":${Re(e.crossover_hz,Ie)}`),e.sub_level_db!==void 0&&(t+=`,"sub_level_db":${Le(e.sub_level_db)}`),e.sub_polarity!==void 0&&(t+=`,"sub_polarity":${JSON.stringify(String(e.sub_polarity))}`),`${t}}`}function jt(s,e){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${q(e)}}`}var x=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),Pe=8;function Ht(s,e=[]){let t=e.map(r=>{let a=x.filter(i=>(r.days??[]).includes(i));return`{"days":${JSON.stringify(a)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${q(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${t.join(",")}]}`}function Bt(s,e){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${e?"true":"false"}}`}function Wt(s,e,t,{stopOnStandby:r=!0,lowLatency:a=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(e)},"enabled":${t?"true":"false"}${r===!1?',"stop_on_standby":false':""}${a===!1?',"low_latency":false':""}}`}var ie=600,oe=720,ne=720,Ce=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function Ue({alarm:s,target:e,time:t,days:r=[],source:a,volume:i,rampS:o,durationMin:u,enabled:d}){let m=x.filter(f=>r.includes(f));return`{"v":2,"t":"alarm_set","alarm":${JSON.stringify(s)},"target":${JSON.stringify(e)},"time":${JSON.stringify(String(t))},"days":${JSON.stringify(m)},"source":${JSON.stringify(a)},"volume":${q(i)},"ramp_s":${Ce(o,ie)},"duration_min":${Ce(u,oe)},"enabled":${d?"true":"false"}}`}function Vt(s){return`{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(s)}}`}function Jt(s){return`{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(s)}}`}function Fe(s,e){return`{"v":2,"t":"sleep","target":${JSON.stringify(s)},"minutes":${Ce(e,ne)}}`}function qt(s,e,t,r){return`{"v":2,"t":"source_store","id":${JSON.stringify(s)},"kind":${JSON.stringify(e)},"value":${JSON.stringify(t)},"name":${JSON.stringify(r)}}`}function Gt(s){return`{"v":2,"t":"source_forget","id":${JSON.stringify(s)}}`}function Kt(s,e){return`{"v":2,"t":"speaker_name","speaker":${JSON.stringify(s)},"name":${JSON.stringify(e)}}`}function Yt(s,e){return`{"v":2,"t":"speaker_room","speaker":${JSON.stringify(s)},"room":${typeof e=="string"&&e?JSON.stringify(e):"null"}}`}function Xt(s){return`{"v":2,"t":"speaker_forget","speaker":${JSON.stringify(s)}}`}function Qt(s,e){return`{"v":2,"t":"firmware_install","speaker":${JSON.stringify(s)},"image":${JSON.stringify(e)}}`}function Zt(s){return`{"v":2,"t":"firmware_cancel","speaker":${JSON.stringify(s)}}`}function er(){return'{"v":2,"t":"firmware_rescan"}'}function De(s,e,t=""){let r=5381;for(let a of String(t))r=(Math.imul(r,33)^a.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(e)}${t?`#${r.toString(36)}`:""}`}function Oe(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var It="Signed out";async function ks(s){let e="";try{e=(await s.text()).trim()}catch{e=""}try{let t=JSON.parse(e);if(t&&typeof t.detail=="string"&&t.detail){let r=typeof t.field=="string"&&t.field?{field:t.field}:{};return{refusal:t.detail,...r}}}catch{}return{refusal:e||`the server answered ${s.status}`}}var ws={set:(s,e)=>globalThis.setTimeout(s,e),clear:s=>globalThis.clearTimeout(s)};function tr({fetch:s=globalThis.fetch.bind(globalThis),base:e="../",timers:t=ws}={}){async function r(){let u=await s(`${e}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(Oe(u))throw Object.assign(new Error(It),{signedOut:!0});if(!u.ok)throw new Error(`the server answered ${u.status}`);return u.json()}async function a(u){let d;try{d=await s(`${e}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:u,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(Oe(d))return{ok:!1,refusal:It,signedOut:!0};if(!d.ok)return{ok:!1,...await ks(d)};try{return{ok:!0,state:await d.json()}}catch{return{ok:!0,state:null}}}function i({onState:u,onStatus:d=()=>{}}){let m=!1,f=null,l=null,p=null,h=()=>{p!==null&&t.clear(p),p=null},v=()=>{h(),p=t.set(()=>f?.abort(),4e4)},y=k=>{let C=k.split(`
`).filter(W=>W.startsWith("data:")).map(W=>W.slice(5).replace(/^ /,"")).join(`
`);if(!C)return;let B;try{B=JSON.parse(C)}catch{return}d("live"),u(B)};async function b(){f=new AbortController,v();let k=!1;try{let C=await s(`${e}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:f.signal});if(k=Oe(C),!C.ok||!C.body)throw new Error(`the server answered ${C.status}`);let B=C.body.getReader();f.signal.addEventListener("abort",()=>B.cancel().catch(()=>{}));let W=new TextDecoder,V="";for(;;){let{done:bs,value:$s}=await B.read();if(bs||m||f.signal.aborted)break;v(),V+=W.decode($s,{stream:!0}).replace(/\r\n?/g,`
`);let Te;for(;(Te=V.indexOf(`

`))!==-1;)y(V.slice(0,Te)),V=V.slice(Te+2)}}catch{}h(),!m&&(d(k?"signed-out":"lost"),l=t.set(()=>{l=null,b()},1e3))}return b(),()=>{m=!0,h(),l!==null&&t.clear(l),f?.abort()}}return{state:r,command:a,events:i,artwork:(u,d)=>De(e,u,d)}}var le=globalThis,de=le.ShadowRoot&&(le.ShadyCSS===void 0||le.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,je=Symbol(),rr=new WeakMap,K=class{constructor(e,t,r){if(this._$cssResult$=!0,r!==je)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=e,this.t=t}get styleSheet(){let e=this.o,t=this.t;if(de&&e===void 0){let r=t!==void 0&&t.length===1;r&&(e=rr.get(t)),e===void 0&&((this.o=e=new CSSStyleSheet).replaceSync(this.cssText),r&&rr.set(t,e))}return e}toString(){return this.cssText}},sr=s=>new K(typeof s=="string"?s:s+"",void 0,je),$=(s,...e)=>{let t=s.length===1?s[0]:e.reduce((r,a,i)=>r+(o=>{if(o._$cssResult$===!0)return o.cssText;if(typeof o=="number")return o;throw Error("Value passed to 'css' function must be a 'css' function result: "+o+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(a)+s[i+1],s[0]);return new K(t,s,je)},ar=(s,e)=>{if(de)s.adoptedStyleSheets=e.map(t=>t instanceof CSSStyleSheet?t:t.styleSheet);else for(let t of e){let r=document.createElement("style"),a=le.litNonce;a!==void 0&&r.setAttribute("nonce",a),r.textContent=t.cssText,s.appendChild(r)}},He=de?s=>s:s=>s instanceof CSSStyleSheet?(e=>{let t="";for(let r of e.cssRules)t+=r.cssText;return sr(t)})(s):s;var{is:Ss,defineProperty:xs,getOwnPropertyDescriptor:As,getOwnPropertyNames:Es,getOwnPropertySymbols:Ts,getPrototypeOf:Os}=Object,ue=globalThis,ir=ue.trustedTypes,Cs=ir?ir.emptyScript:"",Ns=ue.reactiveElementPolyfillSupport,Y=(s,e)=>s,Be={toAttribute(s,e){switch(e){case Boolean:s=s?Cs:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,e){let t=s;switch(e){case Boolean:t=s!==null;break;case Number:t=s===null?null:Number(s);break;case Object:case Array:try{t=JSON.parse(s)}catch{t=null}}return t}},nr=(s,e)=>!Ss(s,e),or={attribute:!0,type:String,converter:Be,reflect:!1,useDefault:!1,hasChanged:nr};Symbol.metadata??=Symbol("metadata"),ue.litPropertyMetadata??=new WeakMap;var E=class extends HTMLElement{static addInitializer(e){this._$Ei(),(this.l??=[]).push(e)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(e,t=or){if(t.state&&(t.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(e)&&((t=Object.create(t)).wrapped=!0),this.elementProperties.set(e,t),!t.noAccessor){let r=Symbol(),a=this.getPropertyDescriptor(e,r,t);a!==void 0&&xs(this.prototype,e,a)}}static getPropertyDescriptor(e,t,r){let{get:a,set:i}=As(this.prototype,e)??{get(){return this[t]},set(o){this[t]=o}};return{get:a,set(o){let u=a?.call(this);i?.call(this,o),this.requestUpdate(e,u,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(e){return this.elementProperties.get(e)??or}static _$Ei(){if(this.hasOwnProperty(Y("elementProperties")))return;let e=Os(this);e.finalize(),e.l!==void 0&&(this.l=[...e.l]),this.elementProperties=new Map(e.elementProperties)}static finalize(){if(this.hasOwnProperty(Y("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(Y("properties"))){let t=this.properties,r=[...Es(t),...Ts(t)];for(let a of r)this.createProperty(a,t[a])}let e=this[Symbol.metadata];if(e!==null){let t=litPropertyMetadata.get(e);if(t!==void 0)for(let[r,a]of t)this.elementProperties.set(r,a)}this._$Eh=new Map;for(let[t,r]of this.elementProperties){let a=this._$Eu(t,r);a!==void 0&&this._$Eh.set(a,t)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(e){let t=[];if(Array.isArray(e)){let r=new Set(e.flat(1/0).reverse());for(let a of r)t.unshift(He(a))}else e!==void 0&&t.push(He(e));return t}static _$Eu(e,t){let r=t.attribute;return r===!1?void 0:typeof r=="string"?r:typeof e=="string"?e.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(e=>this.enableUpdating=e),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(e=>e(this))}addController(e){(this._$EO??=new Set).add(e),this.renderRoot!==void 0&&this.isConnected&&e.hostConnected?.()}removeController(e){this._$EO?.delete(e)}_$E_(){let e=new Map,t=this.constructor.elementProperties;for(let r of t.keys())this.hasOwnProperty(r)&&(e.set(r,this[r]),delete this[r]);e.size>0&&(this._$Ep=e)}createRenderRoot(){let e=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return ar(e,this.constructor.elementStyles),e}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(e=>e.hostConnected?.())}enableUpdating(e){}disconnectedCallback(){this._$EO?.forEach(e=>e.hostDisconnected?.())}attributeChangedCallback(e,t,r){this._$AK(e,r)}_$ET(e,t){let r=this.constructor.elementProperties.get(e),a=this.constructor._$Eu(e,r);if(a!==void 0&&r.reflect===!0){let i=(r.converter?.toAttribute!==void 0?r.converter:Be).toAttribute(t,r.type);this._$Em=e,i==null?this.removeAttribute(a):this.setAttribute(a,i),this._$Em=null}}_$AK(e,t){let r=this.constructor,a=r._$Eh.get(e);if(a!==void 0&&this._$Em!==a){let i=r.getPropertyOptions(a),o=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:Be;this._$Em=a;let u=o.fromAttribute(t,i.type);this[a]=u??this._$Ej?.get(a)??u,this._$Em=null}}requestUpdate(e,t,r,a=!1,i){if(e!==void 0){let o=this.constructor;if(a===!1&&(i=this[e]),r??=o.getPropertyOptions(e),!((r.hasChanged??nr)(i,t)||r.useDefault&&r.reflect&&i===this._$Ej?.get(e)&&!this.hasAttribute(o._$Eu(e,r))))return;this.C(e,t,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(e,t,{useDefault:r,reflect:a,wrapped:i},o){r&&!(this._$Ej??=new Map).has(e)&&(this._$Ej.set(e,o??t??this[e]),i!==!0||o!==void 0)||(this._$AL.has(e)||(this.hasUpdated||r||(t=void 0),this._$AL.set(e,t)),a===!0&&this._$Em!==e&&(this._$Eq??=new Set).add(e))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(t){Promise.reject(t)}let e=this.scheduleUpdate();return e!=null&&await e,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[a,i]of this._$Ep)this[a]=i;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[a,i]of r){let{wrapped:o}=i,u=this[a];o!==!0||this._$AL.has(a)||u===void 0||this.C(a,void 0,i,u)}}let e=!1,t=this._$AL;try{e=this.shouldUpdate(t),e?(this.willUpdate(t),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(t)):this._$EM()}catch(r){throw e=!1,this._$EM(),r}e&&this._$AE(t)}willUpdate(e){}_$AE(e){this._$EO?.forEach(t=>t.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(e)),this.updated(e)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(e){return!0}update(e){this._$Eq&&=this._$Eq.forEach(t=>this._$ET(t,this[t])),this._$EM()}updated(e){}firstUpdated(e){}};E.elementStyles=[],E.shadowRootOptions={mode:"open"},E[Y("elementProperties")]=new Map,E[Y("finalized")]=new Map,Ns?.({ReactiveElement:E}),(ue.reactiveElementVersions??=[]).push("2.1.2");var Ve=globalThis,lr=s=>s,ce=Ve.trustedTypes,dr=ce?ce.createPolicy("lit-html",{createHTML:s=>s}):void 0,Je="$lit$",T=`lit$${Math.random().toFixed(9).slice(2)}$`,qe="?"+T,Rs=`<${qe}>`,I=document,Q=()=>I.createComment(""),Z=s=>s===null||typeof s!="object"&&typeof s!="function",Ge=Array.isArray,fr=s=>Ge(s)||typeof s?.[Symbol.iterator]=="function",We=`[ 	
\f\r]`,X=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,ur=/-->/g,cr=/>/g,R=RegExp(`>|${We}(?:([^\\s"'>=/]+)(${We}*=${We}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),hr=/'/g,pr=/"/g,gr=/^(?:script|style|textarea|title)$/i,Ke=s=>(e,...t)=>({_$litType$:s,strings:e,values:t}),n=Ke(1),Pa=Ke(2),Ua=Ke(3),O=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),mr=new WeakMap,z=I.createTreeWalker(I,129);function vr(s,e){if(!Ge(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return dr!==void 0?dr.createHTML(e):e}var br=(s,e)=>{let t=s.length-1,r=[],a,i=e===2?"<svg>":e===3?"<math>":"",o=X;for(let u=0;u<t;u++){let d=s[u],m,f,l=-1,p=0;for(;p<d.length&&(o.lastIndex=p,f=o.exec(d),f!==null);)p=o.lastIndex,o===X?f[1]==="!--"?o=ur:f[1]!==void 0?o=cr:f[2]!==void 0?(gr.test(f[2])&&(a=RegExp("</"+f[2],"g")),o=R):f[3]!==void 0&&(o=R):o===R?f[0]===">"?(o=a??X,l=-1):f[1]===void 0?l=-2:(l=o.lastIndex-f[2].length,m=f[1],o=f[3]===void 0?R:f[3]==='"'?pr:hr):o===pr||o===hr?o=R:o===ur||o===cr?o=X:(o=R,a=void 0);let h=o===R&&s[u+1].startsWith("/>")?" ":"";i+=o===X?d+Rs:l>=0?(r.push(m),d.slice(0,l)+Je+d.slice(l)+T+h):d+T+(l===-2?u:h)}return[vr(s,i+(s[t]||"<?>")+(e===2?"</svg>":e===3?"</math>":"")),r]},ee=class s{constructor({strings:e,_$litType$:t},r){let a;this.parts=[];let i=0,o=0,u=e.length-1,d=this.parts,[m,f]=br(e,t);if(this.el=s.createElement(m,r),z.currentNode=this.el.content,t===2||t===3){let l=this.el.content.firstChild;l.replaceWith(...l.childNodes)}for(;(a=z.nextNode())!==null&&d.length<u;){if(a.nodeType===1){if(a.hasAttributes())for(let l of a.getAttributeNames())if(l.endsWith(Je)){let p=f[o++],h=a.getAttribute(l).split(T),v=/([.?@])?(.*)/.exec(p);d.push({type:1,index:i,name:v[2],strings:h,ctor:v[1]==="."?pe:v[1]==="?"?me:v[1]==="@"?fe:L}),a.removeAttribute(l)}else l.startsWith(T)&&(d.push({type:6,index:i}),a.removeAttribute(l));if(gr.test(a.tagName)){let l=a.textContent.split(T),p=l.length-1;if(p>0){a.textContent=ce?ce.emptyScript:"";for(let h=0;h<p;h++)a.append(l[h],Q()),z.nextNode(),d.push({type:2,index:++i});a.append(l[p],Q())}}}else if(a.nodeType===8)if(a.data===qe)d.push({type:2,index:i});else{let l=-1;for(;(l=a.data.indexOf(T,l+1))!==-1;)d.push({type:7,index:i}),l+=T.length-1}i++}}static createElement(e,t){let r=I.createElement("template");return r.innerHTML=e,r}};function M(s,e,t=s,r){if(e===O)return e;let a=r!==void 0?t._$Co?.[r]:t._$Cl,i=Z(e)?void 0:e._$litDirective$;return a?.constructor!==i&&(a?._$AO?.(!1),i===void 0?a=void 0:(a=new i(s),a._$AT(s,t,r)),r!==void 0?(t._$Co??=[])[r]=a:t._$Cl=a),a!==void 0&&(e=M(s,a._$AS(s,e.values),a,r)),e}var he=class{constructor(e,t){this._$AV=[],this._$AN=void 0,this._$AD=e,this._$AM=t}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(e){let{el:{content:t},parts:r}=this._$AD,a=(e?.creationScope??I).importNode(t,!0);z.currentNode=a;let i=z.nextNode(),o=0,u=0,d=r[0];for(;d!==void 0;){if(o===d.index){let m;d.type===2?m=new F(i,i.nextSibling,this,e):d.type===1?m=new d.ctor(i,d.name,d.strings,this,e):d.type===6&&(m=new ge(i,this,e)),this._$AV.push(m),d=r[++u]}o!==d?.index&&(i=z.nextNode(),o++)}return z.currentNode=I,a}p(e){let t=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(e,r,t),t+=r.strings.length-2):r._$AI(e[t])),t++}},F=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(e,t,r,a){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=e,this._$AB=t,this._$AM=r,this.options=a,this._$Cv=a?.isConnected??!0}get parentNode(){let e=this._$AA.parentNode,t=this._$AM;return t!==void 0&&e?.nodeType===11&&(e=t.parentNode),e}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(e,t=this){e=M(this,e,t),Z(e)?e===c||e==null||e===""?(this._$AH!==c&&this._$AR(),this._$AH=c):e!==this._$AH&&e!==O&&this._(e):e._$litType$!==void 0?this.$(e):e.nodeType!==void 0?this.T(e):fr(e)?this.k(e):this._(e)}O(e){return this._$AA.parentNode.insertBefore(e,this._$AB)}T(e){this._$AH!==e&&(this._$AR(),this._$AH=this.O(e))}_(e){this._$AH!==c&&Z(this._$AH)?this._$AA.nextSibling.data=e:this.T(I.createTextNode(e)),this._$AH=e}$(e){let{values:t,_$litType$:r}=e,a=typeof r=="number"?this._$AC(e):(r.el===void 0&&(r.el=ee.createElement(vr(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===a)this._$AH.p(t);else{let i=new he(a,this),o=i.u(this.options);i.p(t),this.T(o),this._$AH=i}}_$AC(e){let t=mr.get(e.strings);return t===void 0&&mr.set(e.strings,t=new ee(e)),t}k(e){Ge(this._$AH)||(this._$AH=[],this._$AR());let t=this._$AH,r,a=0;for(let i of e)a===t.length?t.push(r=new s(this.O(Q()),this.O(Q()),this,this.options)):r=t[a],r._$AI(i),a++;a<t.length&&(this._$AR(r&&r._$AB.nextSibling,a),t.length=a)}_$AR(e=this._$AA.nextSibling,t){for(this._$AP?.(!1,!0,t);e!==this._$AB;){let r=lr(e).nextSibling;lr(e).remove(),e=r}}setConnected(e){this._$AM===void 0&&(this._$Cv=e,this._$AP?.(e))}},L=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(e,t,r,a,i){this.type=1,this._$AH=c,this._$AN=void 0,this.element=e,this.name=t,this._$AM=a,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(e,t=this,r,a){let i=this.strings,o=!1;if(i===void 0)e=M(this,e,t,0),o=!Z(e)||e!==this._$AH&&e!==O,o&&(this._$AH=e);else{let u=e,d,m;for(e=i[0],d=0;d<i.length-1;d++)m=M(this,u[r+d],t,d),m===O&&(m=this._$AH[d]),o||=!Z(m)||m!==this._$AH[d],m===c?e=c:e!==c&&(e+=(m??"")+i[d+1]),this._$AH[d]=m}o&&!a&&this.j(e)}j(e){e===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,e??"")}},pe=class extends L{constructor(){super(...arguments),this.type=3}j(e){this.element[this.name]=e===c?void 0:e}},me=class extends L{constructor(){super(...arguments),this.type=4}j(e){this.element.toggleAttribute(this.name,!!e&&e!==c)}},fe=class extends L{constructor(e,t,r,a,i){super(e,t,r,a,i),this.type=5}_$AI(e,t=this){if((e=M(this,e,t,0)??c)===O)return;let r=this._$AH,a=e===c&&r!==c||e.capture!==r.capture||e.once!==r.once||e.passive!==r.passive,i=e!==c&&(r===c||a);a&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,e),this._$AH=e}handleEvent(e){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,e):this._$AH.handleEvent(e)}},ge=class{constructor(e,t,r){this.element=e,this.type=6,this._$AN=void 0,this._$AM=t,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(e){M(this,e)}},$r={M:Je,P:T,A:qe,C:1,L:br,R:he,D:fr,V:M,I:F,H:L,N:me,U:fe,B:pe,F:ge},zs=Ve.litHtmlPolyfillSupport;zs?.(ee,F),(Ve.litHtmlVersions??=[]).push("3.3.3");var _r=(s,e,t)=>{let r=t?.renderBefore??e,a=r._$litPart$;if(a===void 0){let i=t?.renderBefore??null;r._$litPart$=a=new F(e.insertBefore(Q(),i),i,void 0,t??{})}return a._$AI(s),a};var Ye=globalThis,g=class extends E{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let e=super.createRenderRoot();return this.renderOptions.renderBefore??=e.firstChild,e}update(e){let t=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(e),this._$Do=_r(t,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return O}};g._$litElement$=!0,g.finalized=!0,Ye.litElementHydrateSupport?.({LitElement:g});var Is=Ye.litElementPolyfillSupport;Is?.({LitElement:g});(Ye.litElementVersions??=[]).push("4.2.2");function Ms(s,e,t){let r=s.elementFromPoint?.(e,t)??null;for(;r?.shadowRoot?.elementFromPoint;){let a=r.shadowRoot.elementFromPoint(e,t);if(!a||a===r)break;r=a}return r}function Ls(s){for(let e=s;e;e=e.assignedSlot??e.parentNode??e.host){let t=e.dataset?.drop;if(t==="alone")return{kind:t};if((t==="room"||t==="group")&&e.dataset.dropId)return{kind:t,id:e.dataset.dropId}}return null}var yr=(s,e,t)=>Ls(Ms(s,e,t));function kr({root:s=document,onStart:e=()=>{},onOver:t=()=>{},onEnd:r=()=>{}}={}){let a=null,i=()=>{let{handle:l,pointerId:p}=a;l.removeEventListener("pointermove",o),l.removeEventListener("pointerup",u),l.removeEventListener("pointercancel",d),l.removeEventListener("lostpointercapture",d),s.removeEventListener("keydown",m,!0);try{l.releasePointerCapture?.(p)}catch{}a=null};function o(l){if(!(!a||l.pointerId!==a.pointerId)){if(!a.moving){if(Math.hypot(l.clientX-a.x,l.clientY-a.y)<8)return;a.moving=!0,e(a.room)}l.preventDefault(),t(yr(s,l.clientX,l.clientY))}}function u(l){if(!a||l.pointerId!==a.pointerId)return;let{room:p,moving:h}=a;if(i(),!h)return;let v=y=>{y.stopPropagation(),y.preventDefault()};s.addEventListener("click",v,!0),setTimeout(()=>s.removeEventListener("click",v,!0),0),r(p,yr(s,l.clientX,l.clientY))}function d(l){if(!a||l&&l.pointerId!==void 0&&l.pointerId!==a.pointerId)return;let{room:p,moving:h}=a;i(),h&&r(p,null)}function m(l){l.key==="Escape"&&d()}function f(l){if(a||l.isPrimary===!1||l.button>0)return;let p=l.composedPath().find(h=>h.dataset?.dragRoom);if(p){a={handle:p,room:p.dataset.dragRoom,pointerId:l.pointerId,x:l.clientX,y:l.clientY,moving:!1};try{p.setPointerCapture?.(l.pointerId)}catch{}p.addEventListener("pointermove",o),p.addEventListener("pointerup",u),p.addEventListener("pointercancel",d),p.addEventListener("lostpointercapture",d),s.addEventListener("keydown",m,!0)}}return{begin:f,cancel:()=>d(),active:()=>!!a?.moving}}var ve=[],Sr=s=>String(s).split("/").filter(Boolean);function w(s){let{id:e,path:t,title:r,render:a}=s??{};if(typeof e!="string"||!e||e==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof a!="function")throw new Error(`the screen '${e}' has a title and a render`);let i=Sr(t);if(i.length===0)throw new Error(`the screen '${e}' has a path`);let o=u=>u.map(d=>d.startsWith(":")?":":d).join("/");for(let u of ve){if(u.id===e)throw new Error(`the screen '${e}' is registered twice`);if(o(u.segments)===o(i))throw new Error(`the screens '${u.id}' and '${e}' have the same path`)}ve.push({id:e,segments:i,title:r,render:a})}function Xe(s){return ve.find(e=>e.id===s)??null}var $e="#/",be=Object.freeze({screen:"home",params:Object.freeze({}),address:$e});function S(s,e={}){let t=Xe(s);if(!t)throw new Error(`there is no screen '${s}'`);return`#/${t.segments.map(a=>{if(!a.startsWith(":"))return a;let i=e[a.slice(1)];if(typeof i!="string"||!i)throw new Error(`the screen '${s}' needs '${a.slice(1)}'`);return encodeURIComponent(i)}).join("/")}`}function wr(s){let e;try{e=Sr(String(s??"").replace(/^#/,"")).map(t=>decodeURIComponent(t))}catch{return be}for(let t of ve){if(t.segments.length!==e.length)continue;let r={};if(t.segments.every((i,o)=>i.startsWith(":")?(r[i.slice(1)]=e[o],!0):i===e[o]))return{screen:t.id,params:r,address:S(t.id,r)}}return be}function xr(s=globalThis){let e=new Set,t=()=>wr(s.location?.hash??""),r=()=>{let a=t();for(let i of[...e])i(a)};return{route:t,open(a){let i=wr(a);i.address!==t().address&&(s.history.pushState({chorus:!0},"",i.address),r())},back(){if(t().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",$e),r()}},watch(a){let i=o=>a(o);return e.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),e.add(i),i(t()),()=>{e.delete(i),e.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var Ps=(s,e)=>De("../",s,e),_=s=>typeof s=="string"&&s?s:null,Us=["playing","paused","buffering"];function Ar(s,e=Ps){let t=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let a of t){if(!a||typeof a!="object"||typeof a.id!="string"||!a.id)continue;let i=a.now_playing&&typeof a.now_playing=="object"?a.now_playing:null,o=i?_(i.art_url):null;r.set(a.id,{source:_(a.source),nowPlaying:i&&{title:_(i.title),artist:_(i.artist),album:_(i.album),state:Us.includes(i.state)?i.state:null,via:_(i.via),artwork:o?e(a.id,o):null}})}return r}var Ze={source:null,nowPlaying:null};function Fs(s){let e=s&&Array.isArray(s.inputs)?s.inputs:[],t=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return e.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:t.get(r)??r}))}function Ds(s){let e=s&&s.sound&&typeof s.sound=="object"?s.sound:{},t=a=>Number.isInteger(a)?a:null,r=a=>typeof a=="boolean"?a:null;return{bass:t(e.bass),treble:t(e.treble),loudness:r(e.loudness),night:r(e.night),speech:r(e.speech)}}function js(s){let e=s&&typeof s=="object"?s:{},t=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:P(e.limit),effectiveLimit:P(e.effective_limit),quietEnabled:typeof e.quiet_enabled=="boolean"?e.quiet_enabled:null,windows:(Array.isArray(e.quiet)?e.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),start:t(r.start),end:t(r.end),limit:P(r.limit),active:r.active===!0}))}}var Hs=["optical","hdmi_arc"];function Bs(s){return(s&&Array.isArray(s.input_kinds)?s.input_kinds:[]).filter(t=>t&&typeof t.input=="string"&&t.input&&typeof t.kind=="string").filter(t=>typeof t.tv=="boolean"?t.tv:Hs.includes(t.kind)).map(t=>({input:t.input,kind:t.kind}))}var Er=["SL","SR","BL","BR"],Ws=["FC","LFE",...Er];function Vs(s,e=[],t=[]){let r=s&&typeof s=="object"?s:{},a=h=>Number.isInteger(h)?h:null,i=h=>typeof h=="string"&&h?h:null,o=(Array.isArray(r.bond)?r.bond:[]).map(h=>h?.role),u=(Array.isArray(r.endpoints)?r.endpoints:[]).filter(h=>typeof h=="string"&&h),d=h=>u.some(v=>h.startsWith(`${v}/`)),m=h=>t.some(v=>v.input===h&&v.target===r.id),f=e.filter(({input:h})=>d(h)||m(h)),l=r.bass_management&&typeof r.bass_management=="object"?r.bass_management:{},p=o.some(h=>Ws.includes(h));return{offered:f.length>0||p,avTrimMs:a(r.av_trim_ms),tvUpmix:i(r.sound&&typeof r.sound=="object"?r.sound.tv_upmix:null),tvInputs:f,set:p,surrounds:o.some(h=>Er.includes(h)),bass:{crossoverHz:a(l.crossover_hz),subLevel:typeof l.sub_level_db=="number"&&Number.isFinite(l.sub_level_db)?Math.round(l.sub_level_db*100):null,subPolarity:i(l.sub_polarity),active:l.active===!0}}}function tt(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(t=>t&&typeof t.input=="string"&&t.input&&typeof t.target=="string").map(t=>({input:t.input,target:t.target,enabled:t.enabled===!0,stopOnStandby:t.stop_on_standby!==!1,lowLatency:t.low_latency!==!1}))}function Tr(s){let e=s&&Array.isArray(s.alarms)?s.alarms:[],t=r=>Number.isInteger(r)&&r>=0?r:0;return e.filter(r=>r&&typeof r.alarm=="string"&&r.alarm&&typeof r.target=="string").map(r=>({id:r.alarm,target:r.target,time:typeof r.time=="string"?r.time:"",days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),source:typeof r.source=="string"?r.source:"",volume:P(r.volume)??0,rampS:t(r.ramp_s),durationMin:t(r.duration_min),enabled:r.enabled===!0,ringing:r.ringing===!0}))}function Or(s){return(s&&Array.isArray(s.sleep)?s.sleep:[]).filter(t=>t&&typeof t.target=="string"&&t.target).map(t=>({target:t.target,minutes:Number.isInteger(t.minutes)?t.minutes:null,remainingS:Number.isInteger(t.remaining_s)&&t.remaining_s>=0?t.remaining_s:null}))}function Cr(s){return(s&&Array.isArray(s.stored_sources)?s.stored_sources:[]).filter(t=>t&&typeof t.id=="string"&&t.id&&typeof t.kind=="string").map(t=>({id:t.id,kind:t.kind,value:typeof t.value=="string"?t.value:"",name:typeof t.name=="string"&&t.name?t.name:t.id}))}function Nr(s){return!s||!Array.isArray(s.chimes)?null:s.chimes.filter(e=>typeof e=="string"&&e)}function Rr(s){let e=s&&s.soloist&&typeof s.soloist=="object"?s.soloist:null;return e?(Array.isArray(e.receivers)?e.receivers:[]).filter(t=>t&&t.state==="running"&&typeof t.target=="string"&&t.target).map(t=>t.target):null}function rt(s){return(s&&Array.isArray(s.speakers)?s.speakers:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>{let r=t.named===!0,a=_(t.room);return{id:t.id,name:_(t.name)??t.id,named:r,room:a,isNew:!r&&a===null,present:t.present===!0,software:_(t.software),link:_(t.link)??"unknown",key:_(t.key),roles:(Array.isArray(t.roles)?t.roles:[]).filter(i=>typeof i=="string"&&i),firmware:Js(t.firmware)}})}var et=s=>Number.isSafeInteger(s)&&s>0?s:0;function Js(s){if(!s||typeof s!="object")return null;let e=_(s.reason);return{version:_(s.version),board:_(s.board),slot:Number.isSafeInteger(s.slot)?s.slot:null,state:_(s.state)??"idle",reason:e==="none"?null:e,updateAvailable:s.update_available===!0,image:_(s.image),imageVersion:_(s.image_version),received:et(s.received),size:et(s.size)}}function zr(s){let e=s&&s.firmware&&typeof s.firmware=="object"?s.firmware:null;return e?(Array.isArray(e.images)?e.images:[]).filter(t=>t&&typeof t=="object"&&typeof t.name=="string"&&t.name).map(t=>({name:t.name,version:_(t.version),board:_(t.board),size:et(t.size),verified:t.verdict==="verified",reason:_(t.reason)})):null}function Ir(s,e){return!s||!s.updateAvailable||!Array.isArray(e)?[]:e.filter(t=>t.verified&&t.board===s.board&&t.version!==s.version)}function Mr(s){return(s&&Array.isArray(s.key_changes)?s.key_changes:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>({id:t.id,pinned:_(t.pinned),offered:_(t.offered)}))}function A(s,e){return(Array.isArray(s)?s:[]).find(t=>t.id===e)??null}function qs(s,e,t,r,a){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let i=Array.isArray(s.bond)?s.bond:[],o=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:P(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:Ds(s),limits:js(s),theater:Vs(s,r,a),group:o,...o===s.id&&t.get(o)||Ze,bond:i.filter(u=>u&&typeof u.endpoint=="string"&&typeof u.role=="string").map(u=>({endpoint:u.endpoint,name:e.get(u.endpoint)??u.endpoint,role:u.role}))}}function Lr(s,e){let t=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],a=new Map(r.filter(d=>d&&typeof d.id=="string"&&typeof d.name=="string"&&d.name).map(d=>[d.id,d.name])),i=Ar(s,e),o=Bs(s),u=tt(s);return t.map(d=>qs(d,a,i,o,u)).filter(Boolean)}function P(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function Gs(s,e){let t=new Map(Lr(s,e).map(l=>[l.id,l.name])),r=Ar(s,e),a=l=>({id:l,name:t.get(l)??l}),i=l=>Array.isArray(l)?l:[],o=l=>i(l).filter(p=>typeof p=="string"&&p).map(a),u=l=>l&&typeof l=="object"&&typeof l.id=="string"&&l.id,d=i(s?.groups).filter(u),m=i(s?.saved_groups).filter(u),f=new Set(m.map(l=>l.id));return[...m.map(l=>{let p=d.find(h=>h.id===l.id);return{id:l.id,name:typeof l.name=="string"&&l.name?l.name:l.id,kind:"saved",active:l.active===!0,defined:o(l.zones),rooms:p?o(p.zones):[],volume:p?P(p.volume):null,...p&&r.get(l.id)||Ze}}),...d.filter(l=>l.kind==="live"&&!f.has(l.id)).map(l=>{let p=o(l.zones);return{id:l.id,name:p.map(h=>h.name).join(" + ")||l.id,kind:"live",active:null,defined:null,rooms:p,volume:P(l.volume),...r.get(l.id)??Ze}})]}var Qe=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function Pr(s){let e=new Set,t=null,r=[],a=[],i=[],o="connecting",u=!1,d=null,m=()=>({state:t,rooms:r,groups:a,inputs:i,status:o}),f=()=>{let b=m();for(let k of[...e])k(b)},l=b=>{t=b,r=Lr(b,s.artwork),a=Gs(b,s.artwork),i=Fs(b)};function p(){d||(d=s.events({onState(b){Qe(b)&&(u=!0,l(b),f())},onStatus(b){o!==b&&(o=b,f())}}),s.state().then(b=>{u||!Qe(b)||(l(b),f())},()=>{}))}function h(){d?.(),d=null}async function v(b){let k=await s.command(b);return k.signedOut&&o!=="signed-out"&&(o="signed-out",f()),k.ok&&Qe(k.state)&&(!t||k.state.serial>t.serial)&&(l(k.state),f()),k}function y(b){return e.add(b),b(m()),()=>e.delete(b)}return{start:p,stop:h,command:v,subscribe:y,view:m}}var _e=s=>`alarm:${s}`,Ur="alarms:draft",Fr=s=>`stored:${s}`,Dr="stored:draft:",jr=s=>`sleep:${s}`,Hr="sleep:draft:",st={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},ye={url:"Stream URL",spotify:"Spotify URI"},Ks=Object.freeze({alarm:"",target:"",time:"07:00",days:Object.freeze(["mon","tue","wed","thu","fri"]),source:"",volume:300,rampS:30,durationMin:60,enabled:!0}),Ys=Object.freeze({id:"",name:"",kind:"url",value:""}),Xs=Object.freeze({target:"",minutes:30}),at=s=>`${Math.round(s/10)}%`,Qs=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),Br=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function Zs(s){let e=Math.max(0,Math.floor(s)),t=Math.floor(e/3600),r=Math.floor(e%3600/60);return t>0?`${t} h ${r} min left`:r>0?`${r} min ${e%60} s left`:`${e} s left`}var ot=class extends g{static properties={known:{type:Boolean},heard:{attribute:!1},alarms:{attribute:!1},stored:{attribute:!1},sleep:{attribute:!1},chimes:{attribute:!1},receivers:{attribute:!1},inputs:{attribute:!1},rooms:{attribute:!1},savedGroups:{attribute:!1},formedGroups:{attribute:!1},refusals:{attribute:!1},refusalFields:{attribute:!1},_alarm:{state:!0},_source:{state:!0},_timer:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.known=!1,this.heard=null,this.alarms=[],this.stored=[],this.sleep=[],this.chimes=null,this.receivers=null,this.inputs=[],this.rooms=[],this.savedGroups=[],this.formedGroups=[],this.refusals={},this.refusalFields={},this._alarm={...Ks},this._source={...Ys},this._timer={...Xs},this.clock=()=>globalThis.performance.now(),this._heardAt=0,this._ticker=null}disconnectedCallback(){super.disconnectedCallback(),this._tickEvery(!1)}willUpdate(e){e.has("heard")&&(this._heardAt=this.clock())}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-holds]")){let t=e.dataset.holds;e.value!==t&&(e.value=t)}this._tickEvery(this.isConnected&&(this.sleep??[]).some(e=>e.remainingS!==null))}_tickEvery(e){e!==(this._ticker!==null)&&(e?this._ticker=globalThis.setInterval(()=>this.tick(),1e3):(globalThis.clearInterval(this._ticker),this._ticker=null))}tick(){this.requestUpdate()}_left(e){let t=Math.floor(Math.max(0,this.clock()-this._heardAt)/1e3);return Math.max(0,e.remainingS-t)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:e,body:t},bubbles:!0,composed:!0}))}_refusal(e){let t=this.refusals?.[e]??"";if(!t)return n`<p role="alert"></p>`;let r=this.refusalFields?.[e]??"";return n`<p role="alert" data-refusal-field=${r||c}>Refused${r?` (${r})`:""}: ${t}</p>`}_place(e){return[...this.rooms??[],...this.savedGroups??[],...this.formedGroups??[]].find(r=>r.id===e)?.name??e}_storedOf(e){return e.startsWith("stored:")?(this.stored??[]).find(t=>t.id===e.slice(7))??null:null}_sourceName(e){if(e.startsWith("chime:"))return`Chime: ${e.slice(6)}`;if(e.startsWith("line-in:"))return`Input: ${(this.inputs??[]).find(a=>a.source===e)?.label??e.slice(8)}`;let t=this._storedOf(e);return t?`${ye[t.kind]??t.kind}: ${t.name}`:e}_unplayable(e,t){if(e.startsWith("chime:"))return this.chimes!==null&&!this.chimes.includes(e.slice(6))?`This server has no chime "${e.slice(6)}".`:"";if(e.startsWith("line-in:"))return(this.inputs??[]).some(r=>r.source===e)?"":`The input ${e.slice(8)} is not offered now: its speaker is not connected.`;if(e.startsWith("stored:")){let r=this._storedOf(e);if(!r)return`This server has no stored source "${e.slice(7)}".`;if(r.kind!=="spotify")return"";if(this.receivers===null)return"This server runs no Spotify receiver.";let a=(this.savedGroups??[]).some(i=>i.id===t);return this.receivers.includes(`${a?"group":"room"}:${t}`)?"":`No Spotify receiver is running for ${this._place(t)}.`}return"This is not a source an alarm plays."}_alarmOf(e){return(this.alarms??[]).find(t=>t.id===e)??null}_sendable(e,t={}){return Ue({...e,alarm:e.id,...t})}_onSwitch(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);t&&this._ask(_e(t.id),this._sendable(t,{enabled:!t.enabled}))}_onStop(e){let t=e.currentTarget.dataset.alarm;this._ask(_e(t),Jt(t))}_onDelete(e){let t=e.currentTarget.dataset.alarm;this._ask(_e(t),Vt(t))}_onEdit(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);if(!t)return;let{id:r,ringing:a,...i}=t;this._alarm={alarm:r,...i}}_alarmRow(e){let t=e.days.length===0?"once":x.filter(i=>e.days.includes(i)).map(i=>st[i][0]).join(" "),r=e.durationMin===0?"until stopped":`for ${e.durationMin} min`,a=this._unplayable(e.source,e.target);return n`
      <li data-alarm=${e.id} ?data-ringing=${e.ringing}>
        <h4>${e.id}</h4>
        <p data-value="when">${e.time}, ${t}</p>
        <p data-value="what">
          ${this._sourceName(e.source)} in ${this._place(e.target)}, to ${at(e.volume)} over ${e.rampS} s,
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
        ${this._refusal(_e(e.id))}
      </li>
    `}_offeredSources(){let e=t=>(this.stored??[]).filter(r=>r.kind===t).map(r=>({value:`stored:${r.id}`,name:r.name}));return[{kind:"chime",label:"Chimes",options:(this.chimes??[]).map(t=>({value:`chime:${t}`,name:t}))},{kind:"line-in",label:"Inputs",options:(this.inputs??[]).map(t=>({value:t.source,name:t.label}))},{kind:"url",label:"Stored stream URLs",options:e("url")},{kind:"spotify",label:"Stored Spotify URIs",options:e("spotify")}]}_alarmDraft(){let e=this._alarm,t=[...this.rooms??[],...this.savedGroups??[]],r=this._offeredSources().flatMap(a=>a.options)[0];return{...e,target:e.target||(t[0]?.id??""),source:e.source||(r?.value??"")}}_setAlarm(e){this._alarm={...this._alarm,...e}}_onAlarmText(e){this._setAlarm({alarm:e.target.value.trim()})}_onAlarmChoice(e){this._setAlarm({[e.target.dataset.field]:e.target.value})}_onAlarmTime(e){if(!Qs(e.target.value)){e.target.value=this._alarm.time;return}this._setAlarm({time:e.target.value})}_onAlarmDay(e){let t=e.currentTarget.dataset.day,r=this._alarm.days.includes(t)?this._alarm.days.filter(a=>a!==t):x.filter(a=>a===t||this._alarm.days.includes(a));this._setAlarm({days:r})}_onAlarmVolume(e){this._setAlarm({volume:Number(e.target.value)})}_onAlarmCount(e){let{field:t,max:r}=e.target.dataset,a=Br(e.target.value,Number(r));e.target.value=String(a),this._setAlarm({[t]:a})}_onAlarmEnabled(){this._setAlarm({enabled:!this._alarm.enabled})}_onSave(){this._ask(Ur,Ue(this._alarmDraft()))}_kindNotes(e){let t=[];this.chimes===null&&t.push(["chime","This server does not say which chimes it has, so none is offered here."]),(this.inputs??[]).length===0&&t.push(["line-in","No input is offered now: no speaker with a line-in is connected."]);let r=new Set((this.stored??[]).map(o=>o.kind));r.has("url")||t.push(["url","No stream URL is stored: add one under Stored sources."]),r.has("spotify")?this.receivers===null&&t.push(["spotify","This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]):t.push(["spotify","No Spotify URI is stored: add one under Stored sources."]);let a=e.source?this._unplayable(e.source,e.target):"",i=this._storedOf(e.source)?.kind==="spotify"?"spotify":"chosen";return a&&!(i==="spotify"&&this.receivers===null)&&t.push([i,`${a} The alarm would ring the bell chime instead.`]),t.map(([o,u])=>n`<p data-unavailable=${o}>${u}</p>`)}_alarmForm(){let e=this._alarmDraft(),t=this.rooms??[],r=this.savedGroups??[],a=[...t,...r],i=this._offeredSources(),o=l=>n`<option value=${l.value}>${l.name}</option>`,u=l=>n`<option value=${l.id}>${l.name}</option>`,d=i.some(l=>l.options.some(p=>p.value===e.source)),m=this._alarmOf(e.alarm)!==null,f=e.alarm!==""&&e.target!==""&&e.source!=="";return n`
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
            ${a.some(l=>l.id===e.target)||!e.target?c:n`<option value=${e.target}>${e.target} (not on this server now)</option>`}
            ${t.length===0?c:n`<optgroup label="Rooms">${t.map(u)}</optgroup>`}
            ${r.length===0?c:n`<optgroup label="Saved groups">${r.map(u)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${e.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${x.map(l=>n`<button
                type="button"
                data-day=${l}
                aria-label="${st[l][1]}, the alarm"
                aria-pressed=${e.days.includes(l)?"true":"false"}
                @click=${this._onAlarmDay}
              >
                ${st[l][0]}
              </button>`)}
          <p data-value="days">${e.days.length===0?"No day: it rings once, at the next such time.":"It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${e.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${d||!e.source?c:n`<option value=${e.source}>${e.source} (not on this server now)</option>`}
            ${i.map(l=>l.options.length===0?c:n`<optgroup label=${l.label} data-kind=${l.kind}>${l.options.map(o)}</optgroup>`)}
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
            aria-valuetext=${at(e.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${at(e.volume)}</span>
        </div>
        <div class="row">
          <label for="alarm-ramp">Rises over, seconds</label>
          <input
            id="alarm-ramp"
            type="number"
            min="0"
            max=${ie}
            step="1"
            data-field="rampS"
            data-max=${ie}
            .value=${String(e.rampS)}
            aria-label="Alarm ramp, seconds"
            @change=${this._onAlarmCount}
          />
          <label for="alarm-duration">Plays for, minutes</label>
          <input
            id="alarm-duration"
            type="number"
            min="0"
            max=${oe}
            step="1"
            data-field="durationMin"
            data-max=${oe}
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
          <button type="button" aria-label="Save alarm" ?disabled=${!f} @click=${this._onSave}>Save alarm</button>
          <p>${m?`Saving replaces the alarm "${e.alarm}".`:"Nothing is sent until it is saved."}</p>
        </div>
        ${this._refusal(Ur)}
      </div>
    `}_onForget(e){let t=e.currentTarget.dataset.stored;this._ask(Fr(t),Gt(t))}_onSourceField(e){this._source={...this._source,[e.target.dataset.field]:e.target.value.trim()}}_onStore(){let{id:e,kind:t,value:r,name:a}=this._source;this._ask(Dr,qt(e,t,r,a||e))}_storedRow(e){return n`
      <li data-stored=${e.id}>
        <h4>${e.name}</h4>
        <p><span data-value="kind">${ye[e.kind]??e.kind}</span>, <span data-id>${e.id}</span></p>
        <p data-value="value">${e.value}</p>
        <div class="row">
          <button type="button" data-stored=${e.id} aria-label="Forget stored source ${e.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(Fr(e.id))}
      </li>
    `}_storedForm(){let e=this._source,t=e.kind==="spotify";return n`
      <div class="draft" data-draft="stored">
        <div class="row">
          <label for="stored-kind">Kind</label>
          <select id="stored-kind" data-field="kind" data-holds=${e.kind} aria-label="Stored source kind" @change=${this._onSourceField}>
            <option value="url">${ye.url}</option>
            <option value="spotify">${ye.spotify}</option>
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
        ${this._refusal(Dr)}
      </div>
    `}_sleepTargets(){return[...this.rooms??[],...this.formedGroups??[]]}_onCancel(e){let t=e.currentTarget.dataset.target;this._ask(jr(t),Fe(t,0))}_onSleepTarget(e){this._timer={...this._timer,target:e.target.value}}_onSleepMinutes(e){let t=Br(e.target.value,ne);e.target.value=String(t),this._timer={...this._timer,minutes:t}}_onSleep(){let e=this._timer.target||(this._sleepTargets()[0]?.id??"");e&&this._ask(Hr,Fe(e,this._timer.minutes))}_sleepRow(e){let t=this._place(e.target),r=e.remainingS===null?`${e.minutes??"?"} min asked for`:Zs(this._left(e));return n`
      <li data-sleep=${e.target}>
        <h4>${t}</h4>
        <div class="row">
          <span class="figure" data-value="left">${r}</span>
          <button type="button" data-target=${e.target} aria-label="Cancel sleep timer for ${t}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(jr(e.target))}
      </li>
    `}_sleepForm(){let e=this.rooms??[],t=this.formedGroups??[],r=this._timer.target||(this._sleepTargets()[0]?.id??""),a=i=>n`<option value=${i.id}>${i.name}</option>`;return n`
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
            max=${ne}
            step="1"
            .value=${String(this._timer.minutes)}
            aria-label="Sleep timer minutes"
            @change=${this._onSleepMinutes}
          />
          <button type="button" aria-label="Start sleep timer" ?disabled=${!r} @click=${this._onSleep}>Start</button>
        </div>
        <p>It fades the room out and stops it when the time is up. 0 minutes cancels the timer it has.</p>
        ${this._refusal(Hr)}
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
    `}};customElements.define("chorus-alarms",ot);var nt="alarms",it=s=>s.map(({id:e,name:t})=>({id:e,name:t}));w({id:nt,path:"alarms",title:()=>"Alarms and sleep timers",render:(s,{view:e,refusals:t,refusalFields:r})=>{let a=e.groups??[];return n`
      <chorus-alarms
        .known=${e.state!==null}
        .heard=${e.state}
        .alarms=${Tr(e.state)}
        .stored=${Cr(e.state)}
        .sleep=${Or(e.state)}
        .chimes=${Nr(e.state)}
        .receivers=${Rr(e.state)}
        .inputs=${e.inputs??[]}
        .rooms=${it(e.rooms)}
        .savedGroups=${it(a.filter(i=>i.kind==="saved"))}
        .formedGroups=${it(a.filter(i=>i.rooms.length>0))}
        .refusals=${t}
        .refusalFields=${r}
      ></chorus-alarms>
    `}});function te(s,e){return e.find(t=>t.id===s.group&&t.rooms.some(r=>r.id===s.id))??null}function Wr(s,e,t){if(!s||!e)return null;let r=te(s,t);return e.kind==="alone"?r?re(s.id):null:typeof e.id!="string"||!e.id?null:e.kind==="group"?r&&r.id===e.id?null:Ne(s.id,e.id):e.kind==="room"?e.id===s.id||r&&r.rooms.some(a=>a.id===e.id)?null:Ne(s.id,e.id):null}var lt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function Vr(s){if(s==="alone")return{kind:"alone"};let e=String(s).indexOf(":");if(e<1)return null;let t=s.slice(0,e),r=s.slice(e+1);return(t==="room"||t==="group")&&r?{kind:t,id:r}:null}function Jr(s,e){let t=te(s,e);return t?lt({kind:"group",id:t.id}):"alone"}function qr(s,e,t){return[{value:"alone",label:"Alone"},...t.map(r=>({value:lt({kind:"group",id:r.id}),label:r.name})),...e.filter(r=>r.id!==s.id&&!te(r,t)).map(r=>({value:lt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var Gr=s=>`autoplay:${s}`;function ut(s){let e=tt(s.state),t=o=>e.find(u=>u.input===o)??null,r=(s.inputs??[]).map(o=>({input:o.id,label:o.label,offered:!0,rule:t(o.id)})),a=new Set(r.map(o=>o.input)),i=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(o=>o&&typeof o.input=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.input,o.name]));return[...r,...e.filter(o=>!a.has(o.input)).map(o=>({input:o.input,label:i.get(o.input)??o.input,offered:!1,rule:o}))]}var ea={optical:"Optical",hdmi_arc:"HDMI ARC"},ta=[{field:"stopOnStandby",name:"Stop on standby",says:"The TV going to standby stops it at once, with no hold"},{field:"lowLatency",name:"Low latency",says:"Played in low-latency mode when it plays in one wired room; off keeps it on the ordinary path"}],dt=class extends g{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1},tv:{type:Boolean,reflect:!0},home:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={},this.tv=!1,this.home=null}_row(e){return(this.rows??[]).find(t=>t.input===e)??null}_ask(e,t,r,a=e.rule??{}){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Gr(e.input),body:Wt(e.input,t,r,a)},bubbles:!0,composed:!0}))}_onSwitch(e){let t=this._row(e.currentTarget.dataset.input);t&&(t.rule?this._ask(t,t.rule.target,!t.rule.enabled):this.home&&this._ask(t,this.home.id,!0))}_onOption(e){let t=this._row(e.currentTarget.dataset.input),r=e.currentTarget.dataset.option,a=t?.rule?.target??this.home?.id;if(!t||!a)return;let i={stopOnStandby:t.rule?.stopOnStandby??!0,lowLatency:t.rule?.lowLatency??!0};this._ask(t,a,t.rule?.enabled??!1,{...i,[r]:!i[r]})}_onTarget(e){let t=this._row(e.target.dataset.input),r=e.target.value,a=t?.rule?.target??"";e.target.value=a,!(!t||!r||r===a)&&this._ask(t,r,t.rule?.enabled??!1)}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-input]")){let t=this._row(e.dataset.input)?.rule?.target??"";e.value!==t&&(e.value=t)}}_targets(e){let t=this.rooms??[],r=this.groups??[],a=!e||[...t,...r].some(o=>o.id===e.target),i=o=>n`<option value=${o.id} ?selected=${e?.target===o.id}>${o.name}</option>`;return n`
      ${e?c:n`<option value="" selected>Nowhere yet</option>`}
      ${a?c:n`<option value=${e.target} selected>${e.target} (not on this server now)</option>`}
      ${t.length===0?c:n`<optgroup label="Rooms">${t.map(i)}</optgroup>`}
      ${r.length===0?c:n`<optgroup label="Saved groups">${r.map(i)}</optgroup>`}
    `}_input(e){let{input:t,label:r,offered:a,rule:i}=e,o=this.refusals?.[Gr(t)]??"",u=!i&&!!this.home,d=i?i.enabled?"On":"Off":u?`Off: switching it on plays it in ${this.home.name}`:"Choose where it plays, then switch it on";return n`
      <li data-input=${t}>
        <h3>${r}</h3>
        ${r===t?c:n`<p data-id>${t}</p>`}
        ${e.kind?n`<p data-kind>${ea[e.kind]??e.kind}</p>`:c}
        ${a?c:n`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${t}
            aria-label="Autoplay for ${r}"
            aria-pressed=${i?.enabled?"true":"false"}
            ?disabled=${!i&&!u}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${d}</span>
        </div>
        <div class="row">
          <label for="target-${t}">Plays in</label>
          <select id="target-${t}" data-input=${t} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(i)}
          </select>
        </div>
        ${this.tv?ta.map(m=>this._option(e,m,!i&&!u)):c}
        <p role="alert">${o?`Refused: ${o}`:c}</p>
      </li>
    `}_option({input:e,label:t,rule:r},{field:a,name:i,says:o},u){let d=r?.[a]??!0;return n`
      <div class="row">
        <button
          type="button"
          data-input=${e}
          data-option=${a}
          aria-label="${i} for ${t}"
          aria-pressed=${d?"true":"false"}
          ?disabled=${u}
          @click=${this._onOption}
        >
          ${i}
        </button>
        <span data-value=${a}>${d?"On":"Off"}</span>
        <p>${o}</p>
      </div>
    `}render(){if(this.rows===null)return n`<p role="status" data-missing>Reading this server's inputs.</p>`;let e=this.tv?"This room has no TV input now.":"This server offers no input now, and has no autoplay rule.";return n`
      ${this.tv?n`<h3>TV autoplay</h3>`:n`<h2>Autoplay</h2>`}
      <p>
        ${this.tv?"A TV input with a rule that is on plays when the TV's signal arrives.":"An input with a rule that is on plays in its room or its group when its signal arrives."}
      </p>
      ${this.rows.length===0?n`<p role="status" data-none>${e}</p>`:n`<ul aria-label=${this.tv?"TV inputs":"Inputs"}>
            ${this.rows.map(t=>this._input(t))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",dt);var ct="autoplay",Kr=s=>s.map(({id:e,name:t})=>({id:e,name:t}));w({id:ct,path:"autoplay",title:()=>"Autoplay",render:(s,{view:e,refusals:t})=>n`
    <chorus-autoplay
      .rows=${e.state===null?null:ut(e)}
      .rooms=${Kr(e.rooms)}
      .groups=${Kr((e.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${t}
    ></chorus-autoplay>
  `});var Yr={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},ke=s=>(...e)=>({_$litDirective$:s,values:e}),D=class{constructor(e){}get _$AU(){return this._$AM._$AU}_$AT(e,t,r){this._$Ct=e,this._$AM=t,this._$Ci=r}_$AS(e,t){return this.update(e,t)}update(e,t){return this.render(...t)}};var{I:ra}=$r,Xr=s=>s;var Qr=()=>document.createComment(""),j=(s,e,t)=>{let r=s._$AA.parentNode,a=e===void 0?s._$AB:e._$AA;if(t===void 0){let i=r.insertBefore(Qr(),a),o=r.insertBefore(Qr(),a);t=new ra(i,o,s,s.options)}else{let i=t._$AB.nextSibling,o=t._$AM,u=o!==s;if(u){let d;t._$AQ?.(s),t._$AM=s,t._$AP!==void 0&&(d=s._$AU)!==o._$AU&&t._$AP(d)}if(i!==a||u){let d=t._$AA;for(;d!==i;){let m=Xr(d).nextSibling;Xr(r).insertBefore(d,a),d=m}}}return t},N=(s,e,t=s)=>(s._$AI(e,t),s),sa={},we=(s,e=sa)=>s._$AH=e,Zr=s=>s._$AH,Se=s=>{s._$AR(),s._$AA.remove()};var es=(s,e,t)=>{let r=new Map;for(let a=e;a<=t;a++)r.set(s[a],a);return r},xe=ke(class extends D{constructor(s){if(super(s),s.type!==Yr.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,e,t){let r;t===void 0?t=e:e!==void 0&&(r=e);let a=[],i=[],o=0;for(let u of s)a[o]=r?r(u,o):o,i[o]=t(u,o),o++;return{values:i,keys:a}}render(s,e,t){return this.dt(s,e,t).values}update(s,[e,t,r]){let a=Zr(s),{values:i,keys:o}=this.dt(e,t,r);if(!Array.isArray(a))return this.ut=o,i;let u=this.ut??=[],d=[],m,f,l=0,p=a.length-1,h=0,v=i.length-1;for(;l<=p&&h<=v;)if(a[l]===null)l++;else if(a[p]===null)p--;else if(u[l]===o[h])d[h]=N(a[l],i[h]),l++,h++;else if(u[p]===o[v])d[v]=N(a[p],i[v]),p--,v--;else if(u[l]===o[v])d[v]=N(a[l],i[v]),j(s,d[v+1],a[l]),l++,v--;else if(u[p]===o[h])d[h]=N(a[p],i[h]),j(s,a[l],a[p]),p--,h++;else if(m===void 0&&(m=es(o,h,v),f=es(u,l,p)),m.has(u[l]))if(m.has(u[p])){let y=f.get(o[h]),b=y!==void 0?a[y]:null;if(b===null){let k=j(s,a[l]);N(k,i[h]),d[h]=k}else d[h]=N(b,i[h]),j(s,a[l],b),a[y]=null;h++}else Se(a[p]),p--;else Se(a[l]),l++;for(;h<=v;){let y=j(s,d[v+1]);N(y,i[h]),d[h++]=y}for(;l<=p;){let y=a[l++];y!==null&&Se(y)}return this.ut=o,we(s,d),O}});var ts=ke(class extends D{constructor(){super(...arguments),this.key=c}render(s,e){return this.key=s,e}update(s,[e,t]){return e!==this.key&&(we(s),this.key=e),t}});var aa={playing:"Playing",paused:"Paused",buffering:"Buffering"};function ia(s,e=[]){if(!s)return"Unavailable";let t=e.find(o=>o.source===s);if(t)return t.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...a]=s.split(":"),i=a.join(":");return r==="line-in"&&i?`Input ${i}`:r==="player"&&i?`Network player ${i}`:r==="chime"&&i?`Chime ${i}`:r==="soloist"&&i?"Spotify":s}var ht=class extends g{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(e){this._failed=e.target.getAttribute("src")}_onInput(e){e.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:Pt(this.target,e.source)},bubbles:!0,composed:!0}))}_artwork(e){let t=n`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!e.artwork||e.artwork===this._failed?t:ts(e.artwork,n`<img
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
              <p data-state>${aa[e.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${ia(this.source,t)}</p>
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
    `}};customElements.define("chorus-playing",ht);var oa=s=>`${Math.round(s/10)}%`,pt=class extends g{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._slider;if(!t||!this.group||this.group.volume===null)return;let r=e.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(t.value=String(this.group.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(Ut(this.group.id,Number(e.target.value)))}_onActivate(){this._ask(re(this.group.id))}_onRemove(e){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:e.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let e=this.group;return e.kind==="live"?"Live group":e.active?"Saved group, active":e.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let e=this.group,t=new Set(e.rooms.map(i=>i.id)),r=e.defined??[],a=new Set(r.map(i=>i.id));return[...r.map(i=>({...i,playing:t.has(i.id)})),...e.rooms.filter(i=>!a.has(i.id)).map(i=>({...i,playing:!0}))]}render(){let e=this.group;if(!e)return c;let t=e.volume===null?"":oa(this._dragged??e.volume);return n`
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
    `}};customElements.define("chorus-group-card",pt);var mt=class extends g{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
        ${xe(e,r=>r.id,r=>n`<li
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
    `}};customElements.define("chorus-groups",mt);var rs=Object.freeze(["phone","desktop"]),na=48,la=`(min-width: ${na}em)`;function ss(s,e=globalThis){if(typeof e?.matchMedia!="function")return s("phone"),()=>{};let t=e.matchMedia(la),r=()=>s(t.matches?"desktop":"phone");return t.addEventListener("change",r),r(),()=>t.removeEventListener("change",r)}var ft=s=>`limits:${s}`,as={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},da=Object.freeze({days:x,start:"22:00",end:"07:00",limit:250}),U=s=>`${Math.round(s/10)}%`,is=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),ua=({days:s,start:e,end:t,limit:r})=>({days:s,start:e,end:t,limit:r}),gt=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...da},this._held=new Set,this._asked=null,this._unanswered=0}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||t)&&(r.value=r.dataset.server)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:ft(this.room.id),body:e,done:t},bubbles:!0,composed:!0}))}_askWindows(e){let t=this.room.id,r=(this._asked?.room===t?this._asked.windows:this.room.limits.windows).map(ua);e(r),this._asked={room:t,windows:r},this._unanswered+=1,this._ask(Ht(t,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onFocus(e){this._held.add(e.target.dataset.key)}_onBlur(e){let{key:t,server:r}=e.target.dataset;this._held.delete(t),this._release(t),e.target.value=r}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.key]:Number(e.target.value)}}_onLimitChange(e){this._release(e.target.dataset.key),this._ask(jt(this.room.id,Number(e.target.value)))}_onEnabled(){this._ask(Bt(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(e){let t=Number(e.target.dataset.window),r=Number(e.target.value);this._release(e.target.dataset.key),this._askWindows(a=>{a[t]={...a[t],limit:r}})}_onWindowTime(e){let{window:t,edge:r,server:a}=e.target.dataset,i=e.target.value;if(!is(i)){e.target.value=a;return}i!==a&&this._askWindows(o=>{o[Number(t)]={...o[Number(t)],[r]:i}})}_onWindowDay(e){let{window:t,day:r}=e.currentTarget.dataset;this._askWindows(a=>{let i=a[Number(t)],o=i.days.includes(r)?i.days.filter(u=>u!==r):[...i.days,r];a[Number(t)]={...i,days:o}})}_onRemove(e){let t=Number(e.currentTarget.dataset.window);this._askWindows(r=>r.splice(t,1))}_onDraftDay(e){let t=e.currentTarget.dataset.day,r=this._draft.days.includes(t)?this._draft.days.filter(a=>a!==t):x.filter(a=>a===t||this._draft.days.includes(a));this._draft={...this._draft,days:r}}_onDraftTime(e){let t=e.target.dataset.edge;if(!is(e.target.value)){e.target.value=this._draft[t];return}this._draft={...this._draft,[t]:e.target.value}}_onDraftLimit(e){this._draft={...this._draft,limit:Number(e.target.value)}}_onAdd(){this._askWindows(e=>e.push({...this._draft}))}_days(e,t,r,a){let i=this.room;return n`
      <div class="row" role="group" aria-label="Days of ${t} for ${i.name}">
        ${x.map(o=>n`<button
              type="button"
              data-day=${o}
              data-window=${a??c}
              aria-label="${as[o][1]}, ${t} for ${i.name}"
              aria-pressed=${e.includes(o)?"true":"false"}
              @click=${r}
            >
              ${as[o][0]}
            </button>`)}
      </div>
    `}_window(e,t,r){let a=this.room,i=`window ${t+1}`,o=a.limits.quietEnabled!==!1,u=e.active?o?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return n`<li data-window=${t}><p data-value="active">Unavailable</p></li>`;let d=`window-${t}`;return n`
      <li data-window=${t} ?data-active=${e.active}>
        <div class="row">
          <strong>Window ${t+1}</strong>
          <span data-value="active" ?data-active=${e.active&&o}>${u}</span>
        </div>
        ${this._days(e.days,i,this._onWindowDay,t)}
        <div class="row">
          <label for="${d}-start">From</label>
          <input
            id="${d}-start"
            type="time"
            data-key="${d}-start"
            data-window=${t}
            data-edge="start"
            data-server=${e.start}
            aria-label="Start of ${i} for ${a.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
          <label for="${d}-end">Until</label>
          <input
            id="${d}-end"
            type="time"
            data-key="${d}-end"
            data-window=${t}
            data-edge="end"
            data-server=${e.end}
            aria-label="End of ${i} for ${a.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
        </div>
        <div class="row">
          <label for="${d}-limit">Limit</label>
          <input
            id="${d}-limit"
            type="range"
            min="0"
            max="1000"
            step="1"
            data-key="${d}-limit"
            data-window=${t}
            data-server=${e.limit}
            aria-label="Limit of ${i} for ${a.name}"
            aria-valuetext=${U(this._dragged[`${d}-limit`]??e.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${U(this._dragged[`${d}-limit`]??e.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${t} aria-label="Remove ${i} for ${a.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(e){let t=this.room;if(e>=Pe)return n`<p data-full>A room has at most ${Pe} windows. Remove one to add another.</p>`;let r=this._draft,a="the new window";return n`
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
            aria-valuetext=${U(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${U(r.limit)}</span>
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
      </p>`;let{limit:t,effectiveLimit:r,quietEnabled:a,windows:i}=e.limits,o=i.every(m=>m.start&&m.end&&m.limit!==null&&m.days.length>0),u=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c,d=t===null?"Unavailable":U(this._dragged.limit??t);return n`
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
              aria-valuetext=${d}
              @focus=${this._onFocus}
              @blur=${this._onBlur}
              @input=${this._onSliderInput}
              @change=${this._onLimitChange}
            />`}
        <span class="figure" data-value="limit">${d}</span>
      </div>
      <div class="row">
        <span>Limit in force now</span>
        <span class="figure" data-value="effective">${r===null?"Unavailable":U(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${e.volume===null?"Unavailable":U(e.volume)}</span>
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
      ${i.length===0?n`<p data-none>This room has no quiet-hours window.</p>`:c}
      <ol aria-label="Quiet-hours windows of ${e.name}">
        ${i.map((m,f)=>this._window(m,f,o))}
      </ol>
      ${o?n`<h3>Add a window</h3>
            ${this._adding(i.length)}`:n`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${u}</p>
    `}};customElements.define("chorus-room-limits",gt);var vt="room-limits";w({id:vt,path:"rooms/:room/limits",title:({room:s},e)=>`Volume limits of ${A(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-limits
      .room=${A(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[ft(s)]??""}
      .refusalField=${r[ft(s)]??""}
    ></chorus-room-limits>
  `});var ns=Object.freeze(["app","kiosk"]),bt="chorus.kiosk",os="1";function ca(s){let e=new URLSearchParams(s).get("kiosk");return e===null?null:e==="0"||e==="false"?"app":"kiosk"}function ls(s,e){let t=ca(s);try{if(t==="kiosk")e?.setItem(bt,os);else if(t==="app")e?.removeItem(bt);else return e?.getItem(bt)===os?"kiosk":"app"}catch{}return t??"app"}function ds(s=globalThis){try{return s.localStorage??null}catch{return null}}var $t=s=>`sound:${s}`,us=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],ha=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],pa=s=>`${s>0?"+":""}${s} dB`,_t=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(e){return this.renderRoot.querySelector(`input[data-field="${e}"]`)}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of us){let a=this._slider(r),i=this.room.sound[r];!a||i===null||(!this._held.has(r)||t)&&(a.value=String(i))}}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:$t(this.room.id),body:se(this.room.id,{[e]:t})},bubbles:!0,composed:!0}))}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onSliderFocus(e){this._held.add(e.target.dataset.field)}_onSliderBlur(e){let t=e.target.dataset.field;this._held.delete(t),this._release(t);let r=this.room?.sound[t];r!=null&&(e.target.value=String(r))}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.field]:Number(e.target.value)}}_onSliderChange(e){let t=e.target.dataset.field;this._release(t),this._ask(t,Number(e.target.value))}_onSwitch(e){let t=e.currentTarget.dataset.field;this._ask(t,!this.room.sound[t])}_tone({field:e,name:t}){let r=this.room,a=r.sound[e],i=a===null?"Unavailable":pa(this._dragged[e]??a);return n`
      <div class="row">
        <label for=${e}>${t}</label>
        ${a===null?c:n`<input
              id=${e}
              data-field=${e}
              type="range"
              min=${J.min}
              max=${J.max}
              step="1"
              aria-label="${t} for ${r.name}"
              aria-valuetext=${i}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${e}>${i}</span>
      </div>
    `}_switch({field:e,name:t,says:r}){let a=this.room,i=a.sound[e];return n`
      <div class="row">
        <button
          type="button"
          data-field=${e}
          aria-label="${t} for ${a.name}"
          aria-pressed=${i===!0?"true":"false"}
          ?disabled=${i===null}
          @click=${this._onSwitch}
        >
          ${t}
        </button>
        <span data-value=${e}>${i===null?"Unavailable":i?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let e=this.room;if(!e)return n`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let t=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c;return n`
      <h2>Sound of ${e.name}</h2>
      ${us.map(r=>this._tone(r))} ${ha.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${t}</p>
    `}};customElements.define("chorus-room-sound",_t);var yt="room-sound";w({id:yt,path:"rooms/:room/sound",title:({room:s},e)=>`Sound of ${A(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-sound
      .room=${A(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[$t(s)]??""}
      .refusalField=${r[$t(s)]??""}
    ></chorus-room-sound>
  `});var kt=s=>`theater:${s}`,ma=s=>`${s>0?"+":""}${s} ms`,fa=s=>`${s>0?"+":""}${Le(s)} dB`,H={av_trim_ms:{name:"A/V trim",range:G,scale:1,step:1,held:s=>s.theater.avTrimMs,words:ma,command:(s,e)=>ze(s.id,e)},crossover_hz:{name:"Crossover",range:Ie,scale:1,step:1,held:s=>s.theater.bass.crossoverHz,words:s=>`${s} Hz`,command:(s,e)=>ae(s.id,{crossover_hz:e})},sub_level_db:{name:"Sub level",range:Me,scale:100,step:.5,held:s=>s.theater.bass.subLevel,words:fa,command:(s,e)=>ae(s.id,{sub_level_db:e})}},ga={off:"Off",ambient:"Ambient"},va={normal:"Normal",inverted:"Inverted"},wt=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},inputs:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.inputs=[],this.rooms=[],this.groups=[],this.refusals={},this.refusalField="",this._dragged={},this._held=new Set,this._shownRefusal=""}get _refusal(){return this.room&&this.refusals?.[kt(this.room.id)]||""}_position(e,t){return String(t/H[e].scale)}updated(){if(!this.room)return;let e=this._refusal,t=!!e&&e!==this._shownRefusal;this._shownRefusal=e,t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-field]")){let a=r.dataset.field,i=H[a].held(this.room);i!==null&&(!this._held.has(a)||t)&&(r.value=this._position(a,i))}}_send(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:kt(this.room.id),body:e},bubbles:!0,composed:!0}))}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_read(e){return Math.round(Number(e.value)*H[e.dataset.field].scale)}_onSliderFocus(e){this._held.add(e.target.dataset.field)}_onSliderBlur(e){let t=e.target.dataset.field;this._held.delete(t),this._release(t);let r=this.room?H[t].held(this.room):null;r!==null&&(e.target.value=this._position(t,r))}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.field]:this._read(e.target)}}_onSliderChange(e){let t=e.target.dataset.field;this._release(t),this._send(H[t].command(this.room,this._read(e.target)))}_onNudge(e){let t=this.room.theater.avTrimMs;t!==null&&this._send(ze(this.room.id,t+Number(e.currentTarget.dataset.nudge)))}_onUpmix(e){this._send(se(this.room.id,{tv_upmix:e.currentTarget.dataset.choice}))}_onPolarity(e){this._send(ae(this.room.id,{sub_polarity:e.currentTarget.dataset.choice}))}_slider(e){let t=this.room,{name:r,range:a,scale:i,step:o,held:u,words:d}=H[e],m=u(t),f=m===null?"Unavailable":d(this._dragged[e]??m);return n`
      <div class="row">
        <label for=${e}>${r}</label>
        ${m===null?c:n`<input
              id=${e}
              data-field=${e}
              type="range"
              min=${a.min/i}
              max=${a.max/i}
              step=${o}
              aria-label="${r} for ${t.name}"
              aria-valuetext=${f}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${e}>${f}</span>
      </div>
    `}_nudges(){let e=this.room,t=e.theater.avTrimMs,r=(a,i,o)=>n`
      <button
        type="button"
        data-nudge=${a}
        aria-label="A/V trim 1 ms ${i} for ${e.name}"
        ?disabled=${t===null||t+a<G.min||t+a>G.max}
        @click=${this._onNudge}
      >
        ${o}
      </button>
    `;return n`<div class="row">${r(-1,"earlier","1 ms earlier")} ${r(1,"later","1 ms later")}</div>`}_choice({field:e,name:t,words:r,names:a,held:i,onChoose:o}){let u=this.room;return n`
      <div class="row" role="group" aria-label="${t} for ${u.name}">
        <span class="name">${t}</span>
        ${r.map(d=>n`
            <button
              type="button"
              data-choice=${d}
              data-of=${e}
              aria-label="${t} ${a[d].toLowerCase()} for ${u.name}"
              aria-pressed=${i===d?"true":"false"}
              ?disabled=${i===null}
              @click=${o}
            >
              ${a[d]}
            </button>
          `)}
        <span data-value=${e}>${i===null?"Unavailable":a[i]??i}</span>
      </div>
    `}_bass(){let{bass:e}=this.room.theater;return e.active?n`
      ${this._slider("crossover_hz")} ${this._slider("sub_level_db")}
      ${this._choice({field:"sub_polarity",name:"Sub polarity",words:Dt,names:va,held:e.subPolarity,onChoose:this._onPolarity})}
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
      ${this._choice({field:"tv_upmix",name:"TV upmix",words:Ft,names:ga,held:t.tvUpmix,onChoose:this._onUpmix})}
      <h3>Bass management</h3>
      ${this._bass()}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${a}</p>
    `}};customElements.define("chorus-room-theater",wt);var St="room-theater",cs=s=>s.map(({id:e,name:t})=>({id:e,name:t}));function ba(s,e){if(!e)return[];let t=new Map(e.theater.tvInputs.map(({input:r,kind:a})=>[r,a]));return ut(s).filter(r=>t.has(r.input)).map(r=>({...r,kind:t.get(r.input)}))}w({id:St,path:"rooms/:room/theater",title:({room:s},e)=>`Theater of ${A(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-theater
      .room=${A(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .inputs=${ba(e,A(e.rooms,s))}
      .rooms=${cs(e.rooms)}
      .groups=${cs((e.groups??[]).filter(a=>a.kind==="saved"))}
      .refusals=${t}
      .refusalField=${r[kt(s)]??""}
    ></chorus-room-theater>
  `});var $a={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},_a=s=>`${Math.round(s/10)}%`,xt=class extends g{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._list;t&&(t.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let a=e.has("refusal")&&!!this.refusal;a&&(this._dragged=null),(!this._sliderHeld||a)&&(r.value=String(this.room.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(Mt(this.room.id,Number(e.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(e){let t=e.target.value;if(e.target.value=this.place,t===this.place)return;let r=Vr(t);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(Lt(this.room.id,!this.room.muted))}render(){let e=this.room;if(!e)return c;let t=e.volume===null?"Unavailable":_a(this._dragged??e.volume);return n`
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
        <a href=${S(yt,{room:e.id})} data-route aria-label="Sound for ${e.name}">Sound</a>
        <a href=${S(vt,{room:e.id})} data-route aria-label="Limits for ${e.name}">Limits</a>
        ${e.theater?.offered?n`<a href=${S(St,{room:e.id})} data-route aria-label="Theater for ${e.name}">Theater</a>`:c}
      </div>
      ${e.bond.length===0?c:n`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${e.bond.map(r=>n`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${$a[r.role]??r.role}: ${r.name}
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
    `}};customElements.define("chorus-room-card",xt);var At=class extends g{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
        ${xe(e??[],a=>a.id,a=>n`<li
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
                .places=${qr(a,e,t)}
                .place=${Jr(a,t)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",At);var Et="chorus-setup-",Tt=6,ya=12,ka=Object.freeze({form:"GET /",takes:"POST /join",title:"chorus speaker setup"}),wa=Object.freeze([{id:"power",title:"Switch the speaker on",text:[`A Wi-Fi speaker that knows no network raises a Wi-Fi access point of its own, named ${Et} and ${Tt} characters (${Et}<${Tt} characters>).`,`Its setup secret is ${ya} characters and is the access point's password. The speaker prints it, and the address of its join page, on its serial console when the access point comes up.`]},{id:"access-point",title:"Join the speaker's access point",text:[`In this phone's Wi-Fi settings, join the network ${Et}<${Tt} characters> with the setup secret as its password. Accept that it has no internet.`,"The phone is then off the house's network, and this page cannot reach the chorus server until it is back. That is expected. Leave this page open."]},{id:"join-page",title:"Open the speaker's join page",text:[`In the phone's browser, open the address the speaker printed (http://<address>/). The speaker serves the page itself, on its access point: it is titled "${ka.title}" and is a form with two fields.`,"Type the house network's name and its passphrase into that page, and press Join. They go to the speaker and nowhere else: this app never asks for them. The network has to be on 2.4 GHz and have a passphrase; the speaker refuses an open network.",'The page answers "Received". If the join fails the access point stays up: join it again and load the page again, and it says why above the form (auth-error for a wrong passphrase, network-not-found for a name it cannot see).']},{id:"return",title:"Come back to the house's network",text:["The speaker takes its access point down and joins the house's network. The phone goes back to the house's network on its own, or join it again in the Wi-Fi settings.","When the speaker reaches the chorus server it is adopted, and this page says so by itself. There is nothing to press."]}]);function Sa(s,e){let t=new Set(s??[]);return(e??[]).filter(r=>!t.has(r.id))}var Ct="chorus-speaker-setup";function xa(s){try{let e=JSON.parse(s?.getItem(Ct)??"null");return Array.isArray(e)&&e.every(t=>typeof t=="string")?e:null}catch{return null}}function Ot(s,e){try{e===null?s?.removeItem(Ct):s?.setItem(Ct,JSON.stringify(e))}catch{}}var Aa=()=>{try{return globalThis.sessionStorage??null}catch{return null}},Nt=class extends g{static properties={speakers:{attribute:!1},status:{type:String},back:{type:String},storage:{attribute:!1},_baseline:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.status="connecting",this.back="#/",this.storage=Aa(),this._baseline=null}willUpdate(){!this.isConnected||this._baseline!==null||!Array.isArray(this.speakers)||(this._baseline=xa(this.storage)??this.speakers.map(e=>e.id),Ot(this.storage,this._baseline))}disconnectedCallback(){super.disconnectedCallback(),Ot(this.storage,null),this._baseline=null}_onAgain(){Array.isArray(this.speakers)&&(this._baseline=this.speakers.map(e=>e.id),Ot(this.storage,this._baseline))}_status(e){return e.length>0?c:this.status==="lost"||this.status==="signed-out"?n`<p role="status" data-away>
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
    `}render(){let e=this._baseline===null?[]:Sa(this._baseline,this.speakers);return n`
      <h2>Set up a Wi-Fi speaker</h2>
      <p>
        A compact Wi-Fi speaker learns the house's network from a phone, on a page the speaker serves itself. This app
        says the steps and watches for the speaker; it never asks for the network's passphrase.
      </p>
      ${this._done(e)}
      <ol aria-label="Steps" ?data-complete=${e.length>0}>
        ${wa.map((t,r)=>n`
            <li data-step=${t.id}>
              <h3>${r+1}. ${t.title}</h3>
              ${t.text.map(a=>n`<p>${a}</p>`)}
            </li>
          `)}
      </ol>
      ${this._status(e)}
    `}};customElements.define("chorus-speaker-setup",Nt);var hs=s=>`speaker:${s}`,ps="firmware",Ea={wired:"Wired",wireless:"Wi-Fi"},ms=["requested","receiving","verified","pending_verify"],Ta=["requested","receiving"];function Oa(s){let e=s.imageVersion?`version ${s.imageVersion}`:"";return s.image?e?`image ${s.image} (${e})`:`image ${s.image}`:e||"an image"}function Ca(s){let e=Oa(s),t=s.version?`version ${s.version}`:"the version it ran before";switch(s.state){case"idle":return"No install is in progress.";case"requested":return`Install requested: the server is offering ${e} to the speaker.`;case"receiving":return`Receiving ${e}: ${s.received} of ${s.size} bytes.`;case"verified":return`Written and checked: ${e}. The speaker restarts into it.`;case"pending_verify":return`On trial: the speaker runs ${t} and has not confirmed it yet.`;case"confirmed":return`Installed: ${e} confirmed itself, and the speaker runs ${t}.`;case"rolled_back":return`Rolled back: ${e} did not confirm, and the speaker runs ${t} again. Nothing retries it.`;case"refused":return`Refused by the speaker: ${e} was not installed.`;case"interrupted":return`Interrupted: the install of ${e} did not finish and is not resumed. Install again to start over.`;case"cancelled":return`Cancelled: the install of ${e} was abandoned.`;default:return`Firmware state: ${s.state}.`}}var Rt=class extends g{static properties={speakers:{attribute:!1},keyChanges:{attribute:!1},rooms:{attribute:!1},images:{attribute:!1},refusals:{attribute:!1},setup:{type:String},_drafts:{state:!0},_forgetting:{state:!0},_installing:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.keyChanges=[],this.rooms=[],this.refusals={},this.setup="",this.images=null,this._drafts={},this._forgetting=null,this._installing=null}_speaker(e){return(this.speakers??[]).find(t=>t.id===e)??null}willUpdate(e){if(this._installing!==null&&(e.has("speakers")||e.has("images"))){let r=this._speaker(this._installing.speaker);r&&this._offers(r).some(i=>i.name===this._installing.image)||(this._installing=null)}if(!e.has("speakers"))return;let t=Object.entries(this._drafts).filter(([r,a])=>{let i=this._speaker(r);return i&&!(i.named&&i.name===a.trim())});t.length!==Object.keys(this._drafts).length&&(this._drafts=Object.fromEntries(t)),this._forgetting!==null&&!this._speaker(this._forgetting)&&(this._forgetting=null)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:hs(e),body:t},bubbles:!0,composed:!0}))}_onDraft(e){this._drafts={...this._drafts,[e.target.dataset.speaker]:e.target.value}}_name(e){let t=this._speaker(e),r=(this._drafts[e]??t?.name??"").trim();!t||!r||t.named&&r===t.name||this._ask(e,Kt(e,r))}_onName(e){this._name(e.currentTarget.dataset.speaker)}_onNameKey(e){e.key==="Enter"&&(e.preventDefault(),this._name(e.target.dataset.speaker))}_onRoom(e){let t=this._speaker(e.target.dataset.speaker),r=e.target.value,a=t?.room??"";e.target.value=a,!(!t||r===a)&&this._ask(t.id,Yt(t.id,r||null))}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-speaker]")){let t=this._speaker(e.dataset.speaker)?.room??"";e.value!==t&&(e.value=t)}}_onForget(e){let{speaker:t,forget:r}=e.currentTarget.dataset;if(r==="ask"){this._forgetting=t;return}this._forgetting=null,r==="yes"&&this._ask(t,Xt(t))}_offers(e){let t=e.firmware;return!t||this.images===null||ms.includes(t.state)?[]:Ir(t,this.images)}_onInstall(e){let{speaker:t,image:r,install:a}=e.currentTarget.dataset;if(a==="ask"){this._installing={speaker:t,image:r};return}let i=this._installing;this._installing=null,!(a!=="yes"||!i||i.speaker!==t||i.image!==r)&&this._ask(t,Qt(t,r))}_onCancelInstall(e){let{speaker:t}=e.currentTarget.dataset;this._ask(t,Zt(t))}_onRescan(){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:ps,body:er()},bubbles:!0,composed:!0}))}_rooms(e){let t=this.rooms??[],r=e.room===null||t.some(a=>a.id===e.room);return n`
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
    `}_offer(e,t){let{id:r,name:a}=e,i=t.version??"not said";return this._installing?.speaker===r&&this._installing?.image===t.name?n`
      <div data-update=${t.name}>
        <p data-install-question>
          Install image ${t.name} (version ${i}) on ${a} (${r})? It runs version
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
          <span data-update-version>Version ${i}, image ${t.name}</span>
          <button
            type="button"
            data-speaker=${r}
            data-image=${t.name}
            data-install="ask"
            aria-label="Install image ${t.name} (version ${i}) on ${a}"
            ?disabled=${!e.present}
            @click=${this._onInstall}
          >
            Install
          </button>
        </div>
      `}_firmware(e){let t=e.firmware;if(!t)return c;let{id:r,name:a}=e,i=this.images!==null,o=this._offers(e),u=t.state==="receiving"&&t.size>0;return n`
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
          ${Ca(t)}
          ${t.reason?n`<span data-firmware-reason>Reason: ${t.reason}.</span>`:c}
        </p>
        ${u?n`<div class="row">
              <progress
                max=${t.size}
                value=${Math.min(t.received,t.size)}
                aria-label="Install progress of ${a}"
              ></progress>
            </div>`:c}
        ${i&&Ta.includes(t.state)?n`<div class="row">
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
        ${i&&t.updateAvailable?n`
              <p data-update-available>Update available</p>
              ${o.map(d=>this._offer(e,d))}
              ${o.length>0&&!e.present?n`<p data-update-absent>The speaker is not connected: it can be installed when it is.</p>`:c}
              ${o.length===0&&!ms.includes(t.state)?n`<p data-update-unlisted>
                    The server lists no verified image for this board with another version. Rescan the staged images.
                  </p>`:c}
            `:c}
      </section>
    `}_images(){if(this.images===null)return c;let e=this.refusals?.[ps]??"";return n`
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
    `}_row(e){let{id:t,name:r}=e,a=this.refusals?.[hs(t)]??"",i=(this.keyChanges??[]).some(d=>d.id===t),o=this._drafts[t],u=(o??r).trim();return n`
      <li data-speaker=${t} ?data-new=${e.isNew}>
        <h3>${r}</h3>
        ${e.isNew?n`<p data-new-mark>New: adopted, not named and in no room yet.</p>`:c}
        ${i?n`<p data-key-changed>A session under this id offered another key and was refused (above).</p>`:c}
        <p data-id>${t}</p>
        <dl>
          <div>
            <dt>Now</dt>
            <dd data-value="present">${e.present?"Connected":"Not connected"}</dd>
          </div>
          <div>
            <dt>Link</dt>
            <dd data-value="link">${Ea[e.link]??"Not reported"}</dd>
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
            .value=${o??r}
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
    `}};customElements.define("chorus-speakers",Rt);var Ae="speakers",fs="speaker-setup";w({id:Ae,path:"speakers",title:()=>"Speakers",render:(s,{view:e,refusals:t})=>n`
    <chorus-speakers
      .speakers=${e.state===null?null:rt(e.state)}
      .keyChanges=${Mr(e.state)}
      .rooms=${e.rooms.map(({id:r,name:a})=>({id:r,name:a}))}
      .images=${zr(e.state)}
      .refusals=${t}
      .setup=${S(fs)}
    ></chorus-speakers>
  `});w({id:fs,path:"speakers/setup",title:()=>"Set up a Wi-Fi speaker",render:(s,{view:e})=>n`
    <chorus-speaker-setup
      .speakers=${e.state===null?null:rt(e.state)}
      .status=${e.status}
      .back=${S(Ae)}
    ></chorus-speaker-setup>
  `});var zt=class extends g{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=xr(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",e=>this._onLink(e)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=kr({onStart:e=>{let t=this._room(e);t&&(this._moving={id:e,name:t.name,grouped:!!te(t,this._groups)})},onOver:e=>{let t=this._over;t?.kind===e?.kind&&t?.id===e?.id||(this._over=e)},onEnd:(e,t)=>{this._moving=null,this._over=null,t&&this._move(e,t)}})}get _groups(){return this._view.groups??[]}_room(e){return this._view.rooms.find(t=>t.id===e)??null}willUpdate(e){ns.includes(this.mode)||(this.mode="app"),rs.includes(this.layout)||(this.layout="phone"),e.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=ss(e=>{this.layout=e}),this._unroute?.(),this._unroute=this._navigation.watch(e=>{e.address!==this._route.address&&(this._route=e)})}updated(e){if(!e.has("_route")||e.get("_route")===void 0)return;let t=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(t==="groups"?"section":"main");r&&(t&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!t}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(e=>{this._view=e}))}async _send(e,t){if(!this.store)return;this._refusals={...this._refusals,[e]:""},this._refusalFields={...this._refusalFields,[e]:""};let r=await this.store.command(t);r.ok||(this._refusals={...this._refusals,[e]:r.refusal},this._refusalFields={...this._refusalFields,[e]:r.field??""})}_onCommand(e){let{subject:t,room:r,body:a,done:i}=e.detail;this._send(t??r,a).then(()=>i?.())}_move(e,t){let r=this._room(e),a=Wr(r,t,this._groups);a&&this._send(e,a)}_onMove(e){this._move(e.detail.room,e.detail.destination)}_onPointerDown(e){this._drag.begin(e)}_onGo(e){let t=e.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=t,this._navigation.back();return}let r=this.renderRoot.querySelector(t==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(e){if(e.defaultPrevented||e.button>0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;let t=e.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));t&&(e.preventDefault(),t.dataset.route==="back"?this._navigation.back():this._navigation.open(t.getAttribute("href")))}_screen(e){let t=Xe(e.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return n`
      <main
        aria-label=${t.title(e.params,this._view)}
        data-screen=${t.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${$e} data-route="back" aria-label="Back to rooms">Back</a>
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
      ${this._signedOut()} ${this._route.screen===be.screen?this._home():this._screen(this._route)}
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
        <a class="more" href=${S(ct)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${S(nt)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <a class="more" href=${S(Ae)} data-route aria-label="Speakers and their setup">Speakers</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",zt);var Na="sw.js";async function gs(s=globalThis.navigator){let e=s?.serviceWorker;if(!e||typeof e.register!="function")return null;try{return await e.register(Na,{scope:"./",updateViaCache:"none"})}catch{return null}}function vs({navigator:s=globalThis.navigator,document:e=globalThis.document}={}){let t=null;try{t=s?.wakeLock??null}catch{t=null}if(!t||typeof t.request!="function"||typeof e?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,a=null,i=!1,o=async d=>{try{await d.release()}catch{}},u=()=>{i||r||a||e.visibilityState!=="visible"||(a=(async()=>{try{let d=await t.request("screen");if(i){await o(d);return}r=d,d.addEventListener?.("release",()=>{r===d&&(r=null)})}catch{}finally{a=null}})())};return e.addEventListener("visibilitychange",u),u(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;a;)await a},stop:async()=>{for(i=!0,e.removeEventListener("visibilitychange",u);a;)await a;let d=r;r=null,d&&await o(d)}}}var Ee=document.querySelector("chorus-app");if(Ee){Ee.mode=ls(window.location.search,ds(window)),Ee.mode==="kiosk"&&vs();let s=Pr(tr());Ee.store=s,s.start()}gs();
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
