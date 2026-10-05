function q(s){let e=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(e/1e3)}.${String(e%1e3).padStart(3,"0")}`}function yt(s,e){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${q(e)}}`}function kt(s,e){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${e?"true":"false"}}`}function Ee(s,e){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(e)}}`}function ee(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function wt(s,e){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(e)}}`}function St(s,e){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${q(e)}}`}var J=Object.freeze({min:-10,max:10}),as=["bass","treble"],is=["loudness","night","speech"];function At(s,e={}){let t=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of as){if(e[r]===void 0)continue;let a=Math.min(J.max,Math.max(J.min,Math.round(Number(e[r])||0)));t+=`,"${r}":${a}`}for(let r of is)e[r]!==void 0&&(t+=`,"${r}":${e[r]?"true":"false"}`);return`${t}}`}function xt(s,e){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${q(e)}}`}var A=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),Te=8;function Et(s,e=[]){let t=e.map(r=>{let a=A.filter(i=>(r.days??[]).includes(i));return`{"days":${JSON.stringify(a)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${q(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${t.join(",")}]}`}function Tt(s,e){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${e?"true":"false"}}`}function Ot(s,e,t,{stopOnStandby:r=!0,lowLatency:a=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(e)},"enabled":${t?"true":"false"}${r===!1?',"stop_on_standby":false':""}${a===!1?',"low_latency":false':""}}`}var te=600,re=720,se=720,xe=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function Oe({alarm:s,target:e,time:t,days:r=[],source:a,volume:i,rampS:o,durationMin:c,enabled:d}){let p=A.filter(m=>r.includes(m));return`{"v":2,"t":"alarm_set","alarm":${JSON.stringify(s)},"target":${JSON.stringify(e)},"time":${JSON.stringify(String(t))},"days":${JSON.stringify(p)},"source":${JSON.stringify(a)},"volume":${q(i)},"ramp_s":${xe(o,te)},"duration_min":${xe(c,re)},"enabled":${d?"true":"false"}}`}function Ct(s){return`{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(s)}}`}function Nt(s){return`{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(s)}}`}function Ce(s,e){return`{"v":2,"t":"sleep","target":${JSON.stringify(s)},"minutes":${xe(e,se)}}`}function Rt(s,e,t,r){return`{"v":2,"t":"source_store","id":${JSON.stringify(s)},"kind":${JSON.stringify(e)},"value":${JSON.stringify(t)},"name":${JSON.stringify(r)}}`}function zt(s){return`{"v":2,"t":"source_forget","id":${JSON.stringify(s)}}`}function It(s,e){return`{"v":2,"t":"speaker_name","speaker":${JSON.stringify(s)},"name":${JSON.stringify(e)}}`}function Mt(s,e){return`{"v":2,"t":"speaker_room","speaker":${JSON.stringify(s)},"room":${typeof e=="string"&&e?JSON.stringify(e):"null"}}`}function Lt(s){return`{"v":2,"t":"speaker_forget","speaker":${JSON.stringify(s)}}`}function Pt(s,e){return`{"v":2,"t":"firmware_install","speaker":${JSON.stringify(s)},"image":${JSON.stringify(e)}}`}function Ft(s){return`{"v":2,"t":"firmware_cancel","speaker":${JSON.stringify(s)}}`}function Ut(){return'{"v":2,"t":"firmware_rescan"}'}function Ne(s,e,t=""){let r=5381;for(let a of String(t))r=(Math.imul(r,33)^a.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(e)}${t?`#${r.toString(36)}`:""}`}function Ae(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var _t="Signed out";async function os(s){let e="";try{e=(await s.text()).trim()}catch{e=""}try{let t=JSON.parse(e);if(t&&typeof t.detail=="string"&&t.detail){let r=typeof t.field=="string"&&t.field?{field:t.field}:{};return{refusal:t.detail,...r}}}catch{}return{refusal:e||`the server answered ${s.status}`}}var ns={set:(s,e)=>globalThis.setTimeout(s,e),clear:s=>globalThis.clearTimeout(s)};function Dt({fetch:s=globalThis.fetch.bind(globalThis),base:e="../",timers:t=ns}={}){async function r(){let c=await s(`${e}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(Ae(c))throw Object.assign(new Error(_t),{signedOut:!0});if(!c.ok)throw new Error(`the server answered ${c.status}`);return c.json()}async function a(c){let d;try{d=await s(`${e}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:c,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(Ae(d))return{ok:!1,refusal:_t,signedOut:!0};if(!d.ok)return{ok:!1,...await os(d)};try{return{ok:!0,state:await d.json()}}catch{return{ok:!0,state:null}}}function i({onState:c,onStatus:d=()=>{}}){let p=!1,m=null,l=null,h=null,f=()=>{h!==null&&t.clear(h),h=null},b=()=>{f(),h=t.set(()=>m?.abort(),4e4)},y=k=>{let O=k.split(`
`).filter(H=>H.startsWith("data:")).map(H=>H.slice(5).replace(/^ /,"")).join(`
`);if(!O)return;let W;try{W=JSON.parse(O)}catch{return}d("live"),c(W)};async function v(){m=new AbortController,b();let k=!1;try{let O=await s(`${e}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:m.signal});if(k=Ae(O),!O.ok||!O.body)throw new Error(`the server answered ${O.status}`);let W=O.body.getReader();m.signal.addEventListener("abort",()=>W.cancel().catch(()=>{}));let H=new TextDecoder,B="";for(;;){let{done:rs,value:ss}=await W.read();if(rs||p||m.signal.aborted)break;b(),B+=H.decode(ss,{stream:!0}).replace(/\r\n?/g,`
`);let Se;for(;(Se=B.indexOf(`

`))!==-1;)y(B.slice(0,Se)),B=B.slice(Se+2)}}catch{}f(),!p&&(d(k?"signed-out":"lost"),l=t.set(()=>{l=null,v()},1e3))}return v(),()=>{p=!0,f(),l!==null&&t.clear(l),m?.abort()}}return{state:r,command:a,events:i,artwork:(c,d)=>Ne(e,c,d)}}var ae=globalThis,ie=ae.ShadowRoot&&(ae.ShadyCSS===void 0||ae.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,Re=Symbol(),jt=new WeakMap,V=class{constructor(e,t,r){if(this._$cssResult$=!0,r!==Re)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=e,this.t=t}get styleSheet(){let e=this.o,t=this.t;if(ie&&e===void 0){let r=t!==void 0&&t.length===1;r&&(e=jt.get(t)),e===void 0&&((this.o=e=new CSSStyleSheet).replaceSync(this.cssText),r&&jt.set(t,e))}return e}toString(){return this.cssText}},Wt=s=>new V(typeof s=="string"?s:s+"",void 0,Re),$=(s,...e)=>{let t=s.length===1?s[0]:e.reduce((r,a,i)=>r+(o=>{if(o._$cssResult$===!0)return o.cssText;if(typeof o=="number")return o;throw Error("Value passed to 'css' function must be a 'css' function result: "+o+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(a)+s[i+1],s[0]);return new V(t,s,Re)},Ht=(s,e)=>{if(ie)s.adoptedStyleSheets=e.map(t=>t instanceof CSSStyleSheet?t:t.styleSheet);else for(let t of e){let r=document.createElement("style"),a=ae.litNonce;a!==void 0&&r.setAttribute("nonce",a),r.textContent=t.cssText,s.appendChild(r)}},ze=ie?s=>s:s=>s instanceof CSSStyleSheet?(e=>{let t="";for(let r of e.cssRules)t+=r.cssText;return Wt(t)})(s):s;var{is:ls,defineProperty:ds,getOwnPropertyDescriptor:us,getOwnPropertyNames:cs,getOwnPropertySymbols:hs,getPrototypeOf:ps}=Object,oe=globalThis,Bt=oe.trustedTypes,fs=Bt?Bt.emptyScript:"",ms=oe.reactiveElementPolyfillSupport,G=(s,e)=>s,Ie={toAttribute(s,e){switch(e){case Boolean:s=s?fs:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,e){let t=s;switch(e){case Boolean:t=s!==null;break;case Number:t=s===null?null:Number(s);break;case Object:case Array:try{t=JSON.parse(s)}catch{t=null}}return t}},qt=(s,e)=>!ls(s,e),Jt={attribute:!0,type:String,converter:Ie,reflect:!1,useDefault:!1,hasChanged:qt};Symbol.metadata??=Symbol("metadata"),oe.litPropertyMetadata??=new WeakMap;var x=class extends HTMLElement{static addInitializer(e){this._$Ei(),(this.l??=[]).push(e)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(e,t=Jt){if(t.state&&(t.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(e)&&((t=Object.create(t)).wrapped=!0),this.elementProperties.set(e,t),!t.noAccessor){let r=Symbol(),a=this.getPropertyDescriptor(e,r,t);a!==void 0&&ds(this.prototype,e,a)}}static getPropertyDescriptor(e,t,r){let{get:a,set:i}=us(this.prototype,e)??{get(){return this[t]},set(o){this[t]=o}};return{get:a,set(o){let c=a?.call(this);i?.call(this,o),this.requestUpdate(e,c,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(e){return this.elementProperties.get(e)??Jt}static _$Ei(){if(this.hasOwnProperty(G("elementProperties")))return;let e=ps(this);e.finalize(),e.l!==void 0&&(this.l=[...e.l]),this.elementProperties=new Map(e.elementProperties)}static finalize(){if(this.hasOwnProperty(G("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(G("properties"))){let t=this.properties,r=[...cs(t),...hs(t)];for(let a of r)this.createProperty(a,t[a])}let e=this[Symbol.metadata];if(e!==null){let t=litPropertyMetadata.get(e);if(t!==void 0)for(let[r,a]of t)this.elementProperties.set(r,a)}this._$Eh=new Map;for(let[t,r]of this.elementProperties){let a=this._$Eu(t,r);a!==void 0&&this._$Eh.set(a,t)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(e){let t=[];if(Array.isArray(e)){let r=new Set(e.flat(1/0).reverse());for(let a of r)t.unshift(ze(a))}else e!==void 0&&t.push(ze(e));return t}static _$Eu(e,t){let r=t.attribute;return r===!1?void 0:typeof r=="string"?r:typeof e=="string"?e.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(e=>this.enableUpdating=e),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(e=>e(this))}addController(e){(this._$EO??=new Set).add(e),this.renderRoot!==void 0&&this.isConnected&&e.hostConnected?.()}removeController(e){this._$EO?.delete(e)}_$E_(){let e=new Map,t=this.constructor.elementProperties;for(let r of t.keys())this.hasOwnProperty(r)&&(e.set(r,this[r]),delete this[r]);e.size>0&&(this._$Ep=e)}createRenderRoot(){let e=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Ht(e,this.constructor.elementStyles),e}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(e=>e.hostConnected?.())}enableUpdating(e){}disconnectedCallback(){this._$EO?.forEach(e=>e.hostDisconnected?.())}attributeChangedCallback(e,t,r){this._$AK(e,r)}_$ET(e,t){let r=this.constructor.elementProperties.get(e),a=this.constructor._$Eu(e,r);if(a!==void 0&&r.reflect===!0){let i=(r.converter?.toAttribute!==void 0?r.converter:Ie).toAttribute(t,r.type);this._$Em=e,i==null?this.removeAttribute(a):this.setAttribute(a,i),this._$Em=null}}_$AK(e,t){let r=this.constructor,a=r._$Eh.get(e);if(a!==void 0&&this._$Em!==a){let i=r.getPropertyOptions(a),o=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:Ie;this._$Em=a;let c=o.fromAttribute(t,i.type);this[a]=c??this._$Ej?.get(a)??c,this._$Em=null}}requestUpdate(e,t,r,a=!1,i){if(e!==void 0){let o=this.constructor;if(a===!1&&(i=this[e]),r??=o.getPropertyOptions(e),!((r.hasChanged??qt)(i,t)||r.useDefault&&r.reflect&&i===this._$Ej?.get(e)&&!this.hasAttribute(o._$Eu(e,r))))return;this.C(e,t,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(e,t,{useDefault:r,reflect:a,wrapped:i},o){r&&!(this._$Ej??=new Map).has(e)&&(this._$Ej.set(e,o??t??this[e]),i!==!0||o!==void 0)||(this._$AL.has(e)||(this.hasUpdated||r||(t=void 0),this._$AL.set(e,t)),a===!0&&this._$Em!==e&&(this._$Eq??=new Set).add(e))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(t){Promise.reject(t)}let e=this.scheduleUpdate();return e!=null&&await e,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[a,i]of this._$Ep)this[a]=i;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[a,i]of r){let{wrapped:o}=i,c=this[a];o!==!0||this._$AL.has(a)||c===void 0||this.C(a,void 0,i,c)}}let e=!1,t=this._$AL;try{e=this.shouldUpdate(t),e?(this.willUpdate(t),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(t)):this._$EM()}catch(r){throw e=!1,this._$EM(),r}e&&this._$AE(t)}willUpdate(e){}_$AE(e){this._$EO?.forEach(t=>t.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(e)),this.updated(e)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(e){return!0}update(e){this._$Eq&&=this._$Eq.forEach(t=>this._$ET(t,this[t])),this._$EM()}updated(e){}firstUpdated(e){}};x.elementStyles=[],x.shadowRootOptions={mode:"open"},x[G("elementProperties")]=new Map,x[G("finalized")]=new Map,ms?.({ReactiveElement:x}),(oe.reactiveElementVersions??=[]).push("2.1.2");var Le=globalThis,Vt=s=>s,ne=Le.trustedTypes,Gt=ne?ne.createPolicy("lit-html",{createHTML:s=>s}):void 0,Pe="$lit$",E=`lit$${Math.random().toFixed(9).slice(2)}$`,Fe="?"+E,gs=`<${Fe}>`,z=document,Y=()=>z.createComment(""),X=s=>s===null||typeof s!="object"&&typeof s!="function",Ue=Array.isArray,er=s=>Ue(s)||typeof s?.[Symbol.iterator]=="function",Me=`[ 	
\f\r]`,K=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Kt=/-->/g,Yt=/>/g,N=RegExp(`>|${Me}(?:([^\\s"'>=/]+)(${Me}*=${Me}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Xt=/'/g,Qt=/"/g,tr=/^(?:script|style|textarea|title)$/i,De=s=>(e,...t)=>({_$litType$:s,strings:e,values:t}),n=De(1),ca=De(2),ha=De(3),T=Symbol.for("lit-noChange"),u=Symbol.for("lit-nothing"),Zt=new WeakMap,R=z.createTreeWalker(z,129);function rr(s,e){if(!Ue(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return Gt!==void 0?Gt.createHTML(e):e}var sr=(s,e)=>{let t=s.length-1,r=[],a,i=e===2?"<svg>":e===3?"<math>":"",o=K;for(let c=0;c<t;c++){let d=s[c],p,m,l=-1,h=0;for(;h<d.length&&(o.lastIndex=h,m=o.exec(d),m!==null);)h=o.lastIndex,o===K?m[1]==="!--"?o=Kt:m[1]!==void 0?o=Yt:m[2]!==void 0?(tr.test(m[2])&&(a=RegExp("</"+m[2],"g")),o=N):m[3]!==void 0&&(o=N):o===N?m[0]===">"?(o=a??K,l=-1):m[1]===void 0?l=-2:(l=o.lastIndex-m[2].length,p=m[1],o=m[3]===void 0?N:m[3]==='"'?Qt:Xt):o===Qt||o===Xt?o=N:o===Kt||o===Yt?o=K:(o=N,a=void 0);let f=o===N&&s[c+1].startsWith("/>")?" ":"";i+=o===K?d+gs:l>=0?(r.push(p),d.slice(0,l)+Pe+d.slice(l)+E+f):d+E+(l===-2?c:f)}return[rr(s,i+(s[t]||"<?>")+(e===2?"</svg>":e===3?"</math>":"")),r]},Q=class s{constructor({strings:e,_$litType$:t},r){let a;this.parts=[];let i=0,o=0,c=e.length-1,d=this.parts,[p,m]=sr(e,t);if(this.el=s.createElement(p,r),R.currentNode=this.el.content,t===2||t===3){let l=this.el.content.firstChild;l.replaceWith(...l.childNodes)}for(;(a=R.nextNode())!==null&&d.length<c;){if(a.nodeType===1){if(a.hasAttributes())for(let l of a.getAttributeNames())if(l.endsWith(Pe)){let h=m[o++],f=a.getAttribute(l).split(E),b=/([.?@])?(.*)/.exec(h);d.push({type:1,index:i,name:b[2],strings:f,ctor:b[1]==="."?de:b[1]==="?"?ue:b[1]==="@"?ce:M}),a.removeAttribute(l)}else l.startsWith(E)&&(d.push({type:6,index:i}),a.removeAttribute(l));if(tr.test(a.tagName)){let l=a.textContent.split(E),h=l.length-1;if(h>0){a.textContent=ne?ne.emptyScript:"";for(let f=0;f<h;f++)a.append(l[f],Y()),R.nextNode(),d.push({type:2,index:++i});a.append(l[h],Y())}}}else if(a.nodeType===8)if(a.data===Fe)d.push({type:2,index:i});else{let l=-1;for(;(l=a.data.indexOf(E,l+1))!==-1;)d.push({type:7,index:i}),l+=E.length-1}i++}}static createElement(e,t){let r=z.createElement("template");return r.innerHTML=e,r}};function I(s,e,t=s,r){if(e===T)return e;let a=r!==void 0?t._$Co?.[r]:t._$Cl,i=X(e)?void 0:e._$litDirective$;return a?.constructor!==i&&(a?._$AO?.(!1),i===void 0?a=void 0:(a=new i(s),a._$AT(s,t,r)),r!==void 0?(t._$Co??=[])[r]=a:t._$Cl=a),a!==void 0&&(e=I(s,a._$AS(s,e.values),a,r)),e}var le=class{constructor(e,t){this._$AV=[],this._$AN=void 0,this._$AD=e,this._$AM=t}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(e){let{el:{content:t},parts:r}=this._$AD,a=(e?.creationScope??z).importNode(t,!0);R.currentNode=a;let i=R.nextNode(),o=0,c=0,d=r[0];for(;d!==void 0;){if(o===d.index){let p;d.type===2?p=new F(i,i.nextSibling,this,e):d.type===1?p=new d.ctor(i,d.name,d.strings,this,e):d.type===6&&(p=new he(i,this,e)),this._$AV.push(p),d=r[++c]}o!==d?.index&&(i=R.nextNode(),o++)}return R.currentNode=z,a}p(e){let t=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(e,r,t),t+=r.strings.length-2):r._$AI(e[t])),t++}},F=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(e,t,r,a){this.type=2,this._$AH=u,this._$AN=void 0,this._$AA=e,this._$AB=t,this._$AM=r,this.options=a,this._$Cv=a?.isConnected??!0}get parentNode(){let e=this._$AA.parentNode,t=this._$AM;return t!==void 0&&e?.nodeType===11&&(e=t.parentNode),e}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(e,t=this){e=I(this,e,t),X(e)?e===u||e==null||e===""?(this._$AH!==u&&this._$AR(),this._$AH=u):e!==this._$AH&&e!==T&&this._(e):e._$litType$!==void 0?this.$(e):e.nodeType!==void 0?this.T(e):er(e)?this.k(e):this._(e)}O(e){return this._$AA.parentNode.insertBefore(e,this._$AB)}T(e){this._$AH!==e&&(this._$AR(),this._$AH=this.O(e))}_(e){this._$AH!==u&&X(this._$AH)?this._$AA.nextSibling.data=e:this.T(z.createTextNode(e)),this._$AH=e}$(e){let{values:t,_$litType$:r}=e,a=typeof r=="number"?this._$AC(e):(r.el===void 0&&(r.el=Q.createElement(rr(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===a)this._$AH.p(t);else{let i=new le(a,this),o=i.u(this.options);i.p(t),this.T(o),this._$AH=i}}_$AC(e){let t=Zt.get(e.strings);return t===void 0&&Zt.set(e.strings,t=new Q(e)),t}k(e){Ue(this._$AH)||(this._$AH=[],this._$AR());let t=this._$AH,r,a=0;for(let i of e)a===t.length?t.push(r=new s(this.O(Y()),this.O(Y()),this,this.options)):r=t[a],r._$AI(i),a++;a<t.length&&(this._$AR(r&&r._$AB.nextSibling,a),t.length=a)}_$AR(e=this._$AA.nextSibling,t){for(this._$AP?.(!1,!0,t);e!==this._$AB;){let r=Vt(e).nextSibling;Vt(e).remove(),e=r}}setConnected(e){this._$AM===void 0&&(this._$Cv=e,this._$AP?.(e))}},M=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(e,t,r,a,i){this.type=1,this._$AH=u,this._$AN=void 0,this.element=e,this.name=t,this._$AM=a,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=u}_$AI(e,t=this,r,a){let i=this.strings,o=!1;if(i===void 0)e=I(this,e,t,0),o=!X(e)||e!==this._$AH&&e!==T,o&&(this._$AH=e);else{let c=e,d,p;for(e=i[0],d=0;d<i.length-1;d++)p=I(this,c[r+d],t,d),p===T&&(p=this._$AH[d]),o||=!X(p)||p!==this._$AH[d],p===u?e=u:e!==u&&(e+=(p??"")+i[d+1]),this._$AH[d]=p}o&&!a&&this.j(e)}j(e){e===u?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,e??"")}},de=class extends M{constructor(){super(...arguments),this.type=3}j(e){this.element[this.name]=e===u?void 0:e}},ue=class extends M{constructor(){super(...arguments),this.type=4}j(e){this.element.toggleAttribute(this.name,!!e&&e!==u)}},ce=class extends M{constructor(e,t,r,a,i){super(e,t,r,a,i),this.type=5}_$AI(e,t=this){if((e=I(this,e,t,0)??u)===T)return;let r=this._$AH,a=e===u&&r!==u||e.capture!==r.capture||e.once!==r.once||e.passive!==r.passive,i=e!==u&&(r===u||a);a&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,e),this._$AH=e}handleEvent(e){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,e):this._$AH.handleEvent(e)}},he=class{constructor(e,t,r){this.element=e,this.type=6,this._$AN=void 0,this._$AM=t,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(e){I(this,e)}},ar={M:Pe,P:E,A:Fe,C:1,L:sr,R:le,D:er,V:I,I:F,H:M,N:ue,U:ce,B:de,F:he},vs=Le.litHtmlPolyfillSupport;vs?.(Q,F),(Le.litHtmlVersions??=[]).push("3.3.3");var ir=(s,e,t)=>{let r=t?.renderBefore??e,a=r._$litPart$;if(a===void 0){let i=t?.renderBefore??null;r._$litPart$=a=new F(e.insertBefore(Y(),i),i,void 0,t??{})}return a._$AI(s),a};var je=globalThis,g=class extends x{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let e=super.createRenderRoot();return this.renderOptions.renderBefore??=e.firstChild,e}update(e){let t=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(e),this._$Do=ir(t,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return T}};g._$litElement$=!0,g.finalized=!0,je.litElementHydrateSupport?.({LitElement:g});var bs=je.litElementPolyfillSupport;bs?.({LitElement:g});(je.litElementVersions??=[]).push("4.2.2");function $s(s,e,t){let r=s.elementFromPoint?.(e,t)??null;for(;r?.shadowRoot?.elementFromPoint;){let a=r.shadowRoot.elementFromPoint(e,t);if(!a||a===r)break;r=a}return r}function _s(s){for(let e=s;e;e=e.assignedSlot??e.parentNode??e.host){let t=e.dataset?.drop;if(t==="alone")return{kind:t};if((t==="room"||t==="group")&&e.dataset.dropId)return{kind:t,id:e.dataset.dropId}}return null}var or=(s,e,t)=>_s($s(s,e,t));function nr({root:s=document,onStart:e=()=>{},onOver:t=()=>{},onEnd:r=()=>{}}={}){let a=null,i=()=>{let{handle:l,pointerId:h}=a;l.removeEventListener("pointermove",o),l.removeEventListener("pointerup",c),l.removeEventListener("pointercancel",d),l.removeEventListener("lostpointercapture",d),s.removeEventListener("keydown",p,!0);try{l.releasePointerCapture?.(h)}catch{}a=null};function o(l){if(!(!a||l.pointerId!==a.pointerId)){if(!a.moving){if(Math.hypot(l.clientX-a.x,l.clientY-a.y)<8)return;a.moving=!0,e(a.room)}l.preventDefault(),t(or(s,l.clientX,l.clientY))}}function c(l){if(!a||l.pointerId!==a.pointerId)return;let{room:h,moving:f}=a;if(i(),!f)return;let b=y=>{y.stopPropagation(),y.preventDefault()};s.addEventListener("click",b,!0),setTimeout(()=>s.removeEventListener("click",b,!0),0),r(h,or(s,l.clientX,l.clientY))}function d(l){if(!a||l&&l.pointerId!==void 0&&l.pointerId!==a.pointerId)return;let{room:h,moving:f}=a;i(),f&&r(h,null)}function p(l){l.key==="Escape"&&d()}function m(l){if(a||l.isPrimary===!1||l.button>0)return;let h=l.composedPath().find(f=>f.dataset?.dragRoom);if(h){a={handle:h,room:h.dataset.dragRoom,pointerId:l.pointerId,x:l.clientX,y:l.clientY,moving:!1};try{h.setPointerCapture?.(l.pointerId)}catch{}h.addEventListener("pointermove",o),h.addEventListener("pointerup",c),h.addEventListener("pointercancel",d),h.addEventListener("lostpointercapture",d),s.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>d(),active:()=>!!a?.moving}}var pe=[],dr=s=>String(s).split("/").filter(Boolean);function w(s){let{id:e,path:t,title:r,render:a}=s??{};if(typeof e!="string"||!e||e==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof a!="function")throw new Error(`the screen '${e}' has a title and a render`);let i=dr(t);if(i.length===0)throw new Error(`the screen '${e}' has a path`);let o=c=>c.map(d=>d.startsWith(":")?":":d).join("/");for(let c of pe){if(c.id===e)throw new Error(`the screen '${e}' is registered twice`);if(o(c.segments)===o(i))throw new Error(`the screens '${c.id}' and '${e}' have the same path`)}pe.push({id:e,segments:i,title:r,render:a})}function We(s){return pe.find(e=>e.id===s)??null}var me="#/",fe=Object.freeze({screen:"home",params:Object.freeze({}),address:me});function S(s,e={}){let t=We(s);if(!t)throw new Error(`there is no screen '${s}'`);return`#/${t.segments.map(a=>{if(!a.startsWith(":"))return a;let i=e[a.slice(1)];if(typeof i!="string"||!i)throw new Error(`the screen '${s}' needs '${a.slice(1)}'`);return encodeURIComponent(i)}).join("/")}`}function lr(s){let e;try{e=dr(String(s??"").replace(/^#/,"")).map(t=>decodeURIComponent(t))}catch{return fe}for(let t of pe){if(t.segments.length!==e.length)continue;let r={};if(t.segments.every((i,o)=>i.startsWith(":")?(r[i.slice(1)]=e[o],!0):i===e[o]))return{screen:t.id,params:r,address:S(t.id,r)}}return fe}function ur(s=globalThis){let e=new Set,t=()=>lr(s.location?.hash??""),r=()=>{let a=t();for(let i of[...e])i(a)};return{route:t,open(a){let i=lr(a);i.address!==t().address&&(s.history.pushState({chorus:!0},"",i.address),r())},back(){if(t().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",me),r()}},watch(a){let i=o=>a(o);return e.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),e.add(i),i(t()),()=>{e.delete(i),e.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var ys=(s,e)=>Ne("../",s,e),_=s=>typeof s=="string"&&s?s:null,ks=["playing","paused","buffering"];function cr(s,e=ys){let t=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let a of t){if(!a||typeof a!="object"||typeof a.id!="string"||!a.id)continue;let i=a.now_playing&&typeof a.now_playing=="object"?a.now_playing:null,o=i?_(i.art_url):null;r.set(a.id,{source:_(a.source),nowPlaying:i&&{title:_(i.title),artist:_(i.artist),album:_(i.album),state:ks.includes(i.state)?i.state:null,via:_(i.via),artwork:o?e(a.id,o):null}})}return r}var Be={source:null,nowPlaying:null};function ws(s){let e=s&&Array.isArray(s.inputs)?s.inputs:[],t=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return e.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:t.get(r)??r}))}function Ss(s){let e=s&&s.sound&&typeof s.sound=="object"?s.sound:{},t=a=>Number.isInteger(a)?a:null,r=a=>typeof a=="boolean"?a:null;return{bass:t(e.bass),treble:t(e.treble),loudness:r(e.loudness),night:r(e.night),speech:r(e.speech)}}function As(s){let e=s&&typeof s=="object"?s:{},t=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:L(e.limit),effectiveLimit:L(e.effective_limit),quietEnabled:typeof e.quiet_enabled=="boolean"?e.quiet_enabled:null,windows:(Array.isArray(e.quiet)?e.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),start:t(r.start),end:t(r.end),limit:L(r.limit),active:r.active===!0}))}}function hr(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(t=>t&&typeof t.input=="string"&&t.input&&typeof t.target=="string").map(t=>({input:t.input,target:t.target,enabled:t.enabled===!0,stopOnStandby:t.stop_on_standby!==!1,lowLatency:t.low_latency!==!1}))}function pr(s){let e=s&&Array.isArray(s.alarms)?s.alarms:[],t=r=>Number.isInteger(r)&&r>=0?r:0;return e.filter(r=>r&&typeof r.alarm=="string"&&r.alarm&&typeof r.target=="string").map(r=>({id:r.alarm,target:r.target,time:typeof r.time=="string"?r.time:"",days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),source:typeof r.source=="string"?r.source:"",volume:L(r.volume)??0,rampS:t(r.ramp_s),durationMin:t(r.duration_min),enabled:r.enabled===!0,ringing:r.ringing===!0}))}function fr(s){return(s&&Array.isArray(s.sleep)?s.sleep:[]).filter(t=>t&&typeof t.target=="string"&&t.target).map(t=>({target:t.target,minutes:Number.isInteger(t.minutes)?t.minutes:null,remainingS:Number.isInteger(t.remaining_s)&&t.remaining_s>=0?t.remaining_s:null}))}function mr(s){return(s&&Array.isArray(s.stored_sources)?s.stored_sources:[]).filter(t=>t&&typeof t.id=="string"&&t.id&&typeof t.kind=="string").map(t=>({id:t.id,kind:t.kind,value:typeof t.value=="string"?t.value:"",name:typeof t.name=="string"&&t.name?t.name:t.id}))}function gr(s){return!s||!Array.isArray(s.chimes)?null:s.chimes.filter(e=>typeof e=="string"&&e)}function vr(s){let e=s&&s.soloist&&typeof s.soloist=="object"?s.soloist:null;return e?(Array.isArray(e.receivers)?e.receivers:[]).filter(t=>t&&t.state==="running"&&typeof t.target=="string"&&t.target).map(t=>t.target):null}function qe(s){return(s&&Array.isArray(s.speakers)?s.speakers:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>{let r=t.named===!0,a=_(t.room);return{id:t.id,name:_(t.name)??t.id,named:r,room:a,isNew:!r&&a===null,present:t.present===!0,software:_(t.software),link:_(t.link)??"unknown",key:_(t.key),roles:(Array.isArray(t.roles)?t.roles:[]).filter(i=>typeof i=="string"&&i),firmware:xs(t.firmware)}})}var Je=s=>Number.isSafeInteger(s)&&s>0?s:0;function xs(s){if(!s||typeof s!="object")return null;let e=_(s.reason);return{version:_(s.version),board:_(s.board),slot:Number.isSafeInteger(s.slot)?s.slot:null,state:_(s.state)??"idle",reason:e==="none"?null:e,updateAvailable:s.update_available===!0,image:_(s.image),imageVersion:_(s.image_version),received:Je(s.received),size:Je(s.size)}}function br(s){let e=s&&s.firmware&&typeof s.firmware=="object"?s.firmware:null;return e?(Array.isArray(e.images)?e.images:[]).filter(t=>t&&typeof t=="object"&&typeof t.name=="string"&&t.name).map(t=>({name:t.name,version:_(t.version),board:_(t.board),size:Je(t.size),verified:t.verdict==="verified",reason:_(t.reason)})):null}function $r(s,e){return!s||!s.updateAvailable||!Array.isArray(e)?[]:e.filter(t=>t.verified&&t.board===s.board&&t.version!==s.version)}function _r(s){return(s&&Array.isArray(s.key_changes)?s.key_changes:[]).filter(t=>t&&typeof t=="object"&&typeof t.id=="string"&&t.id).map(t=>({id:t.id,pinned:_(t.pinned),offered:_(t.offered)}))}function U(s,e){return(Array.isArray(s)?s:[]).find(t=>t.id===e)??null}function Es(s,e,t){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let r=Array.isArray(s.bond)?s.bond:[],a=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:L(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:Ss(s),limits:As(s),group:a,...a===s.id&&t.get(a)||Be,bond:r.filter(i=>i&&typeof i.endpoint=="string"&&typeof i.role=="string").map(i=>({endpoint:i.endpoint,name:e.get(i.endpoint)??i.endpoint,role:i.role}))}}function yr(s,e){let t=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],a=new Map(r.filter(o=>o&&typeof o.id=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.id,o.name])),i=cr(s,e);return t.map(o=>Es(o,a,i)).filter(Boolean)}function L(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function Ts(s,e){let t=new Map(yr(s,e).map(l=>[l.id,l.name])),r=cr(s,e),a=l=>({id:l,name:t.get(l)??l}),i=l=>Array.isArray(l)?l:[],o=l=>i(l).filter(h=>typeof h=="string"&&h).map(a),c=l=>l&&typeof l=="object"&&typeof l.id=="string"&&l.id,d=i(s?.groups).filter(c),p=i(s?.saved_groups).filter(c),m=new Set(p.map(l=>l.id));return[...p.map(l=>{let h=d.find(f=>f.id===l.id);return{id:l.id,name:typeof l.name=="string"&&l.name?l.name:l.id,kind:"saved",active:l.active===!0,defined:o(l.zones),rooms:h?o(h.zones):[],volume:h?L(h.volume):null,...h&&r.get(l.id)||Be}}),...d.filter(l=>l.kind==="live"&&!m.has(l.id)).map(l=>{let h=o(l.zones);return{id:l.id,name:h.map(f=>f.name).join(" + ")||l.id,kind:"live",active:null,defined:null,rooms:h,volume:L(l.volume),...r.get(l.id)??Be}})]}var He=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function kr(s){let e=new Set,t=null,r=[],a=[],i=[],o="connecting",c=!1,d=null,p=()=>({state:t,rooms:r,groups:a,inputs:i,status:o}),m=()=>{let v=p();for(let k of[...e])k(v)},l=v=>{t=v,r=yr(v,s.artwork),a=Ts(v,s.artwork),i=ws(v)};function h(){d||(d=s.events({onState(v){He(v)&&(c=!0,l(v),m())},onStatus(v){o!==v&&(o=v,m())}}),s.state().then(v=>{c||!He(v)||(l(v),m())},()=>{}))}function f(){d?.(),d=null}async function b(v){let k=await s.command(v);return k.signedOut&&o!=="signed-out"&&(o="signed-out",m()),k.ok&&He(k.state)&&(!t||k.state.serial>t.serial)&&(l(k.state),m()),k}function y(v){return e.add(v),v(p()),()=>e.delete(v)}return{start:h,stop:f,command:b,subscribe:y,view:p}}var ge=s=>`alarm:${s}`,wr="alarms:draft",Sr=s=>`stored:${s}`,Ar="stored:draft:",xr=s=>`sleep:${s}`,Er="sleep:draft:",Ve={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},ve={url:"Stream URL",spotify:"Spotify URI"},Os=Object.freeze({alarm:"",target:"",time:"07:00",days:Object.freeze(["mon","tue","wed","thu","fri"]),source:"",volume:300,rampS:30,durationMin:60,enabled:!0}),Cs=Object.freeze({id:"",name:"",kind:"url",value:""}),Ns=Object.freeze({target:"",minutes:30}),Ge=s=>`${Math.round(s/10)}%`,Rs=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),Tr=(s,e)=>Math.min(e,Math.max(0,Math.round(Number(s)||0)));function zs(s){let e=Math.max(0,Math.floor(s)),t=Math.floor(e/3600),r=Math.floor(e%3600/60);return t>0?`${t} h ${r} min left`:r>0?`${r} min ${e%60} s left`:`${e} s left`}var Ye=class extends g{static properties={known:{type:Boolean},heard:{attribute:!1},alarms:{attribute:!1},stored:{attribute:!1},sleep:{attribute:!1},chimes:{attribute:!1},receivers:{attribute:!1},inputs:{attribute:!1},rooms:{attribute:!1},savedGroups:{attribute:!1},formedGroups:{attribute:!1},refusals:{attribute:!1},refusalFields:{attribute:!1},_alarm:{state:!0},_source:{state:!0},_timer:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.known=!1,this.heard=null,this.alarms=[],this.stored=[],this.sleep=[],this.chimes=null,this.receivers=null,this.inputs=[],this.rooms=[],this.savedGroups=[],this.formedGroups=[],this.refusals={},this.refusalFields={},this._alarm={...Os},this._source={...Cs},this._timer={...Ns},this.clock=()=>globalThis.performance.now(),this._heardAt=0,this._ticker=null}disconnectedCallback(){super.disconnectedCallback(),this._tickEvery(!1)}willUpdate(e){e.has("heard")&&(this._heardAt=this.clock())}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-holds]")){let t=e.dataset.holds;e.value!==t&&(e.value=t)}this._tickEvery(this.isConnected&&(this.sleep??[]).some(e=>e.remainingS!==null))}_tickEvery(e){e!==(this._ticker!==null)&&(e?this._ticker=globalThis.setInterval(()=>this.tick(),1e3):(globalThis.clearInterval(this._ticker),this._ticker=null))}tick(){this.requestUpdate()}_left(e){let t=Math.floor(Math.max(0,this.clock()-this._heardAt)/1e3);return Math.max(0,e.remainingS-t)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:e,body:t},bubbles:!0,composed:!0}))}_refusal(e){let t=this.refusals?.[e]??"";if(!t)return n`<p role="alert"></p>`;let r=this.refusalFields?.[e]??"";return n`<p role="alert" data-refusal-field=${r||u}>Refused${r?` (${r})`:""}: ${t}</p>`}_place(e){return[...this.rooms??[],...this.savedGroups??[],...this.formedGroups??[]].find(r=>r.id===e)?.name??e}_storedOf(e){return e.startsWith("stored:")?(this.stored??[]).find(t=>t.id===e.slice(7))??null:null}_sourceName(e){if(e.startsWith("chime:"))return`Chime: ${e.slice(6)}`;if(e.startsWith("line-in:"))return`Input: ${(this.inputs??[]).find(a=>a.source===e)?.label??e.slice(8)}`;let t=this._storedOf(e);return t?`${ve[t.kind]??t.kind}: ${t.name}`:e}_unplayable(e,t){if(e.startsWith("chime:"))return this.chimes!==null&&!this.chimes.includes(e.slice(6))?`This server has no chime "${e.slice(6)}".`:"";if(e.startsWith("line-in:"))return(this.inputs??[]).some(r=>r.source===e)?"":`The input ${e.slice(8)} is not offered now: its speaker is not connected.`;if(e.startsWith("stored:")){let r=this._storedOf(e);if(!r)return`This server has no stored source "${e.slice(7)}".`;if(r.kind!=="spotify")return"";if(this.receivers===null)return"This server runs no Spotify receiver.";let a=(this.savedGroups??[]).some(i=>i.id===t);return this.receivers.includes(`${a?"group":"room"}:${t}`)?"":`No Spotify receiver is running for ${this._place(t)}.`}return"This is not a source an alarm plays."}_alarmOf(e){return(this.alarms??[]).find(t=>t.id===e)??null}_sendable(e,t={}){return Oe({...e,alarm:e.id,...t})}_onSwitch(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);t&&this._ask(ge(t.id),this._sendable(t,{enabled:!t.enabled}))}_onStop(e){let t=e.currentTarget.dataset.alarm;this._ask(ge(t),Nt(t))}_onDelete(e){let t=e.currentTarget.dataset.alarm;this._ask(ge(t),Ct(t))}_onEdit(e){let t=this._alarmOf(e.currentTarget.dataset.alarm);if(!t)return;let{id:r,ringing:a,...i}=t;this._alarm={alarm:r,...i}}_alarmRow(e){let t=e.days.length===0?"once":A.filter(i=>e.days.includes(i)).map(i=>Ve[i][0]).join(" "),r=e.durationMin===0?"until stopped":`for ${e.durationMin} min`,a=this._unplayable(e.source,e.target);return n`
      <li data-alarm=${e.id} ?data-ringing=${e.ringing}>
        <h4>${e.id}</h4>
        <p data-value="when">${e.time}, ${t}</p>
        <p data-value="what">
          ${this._sourceName(e.source)} in ${this._place(e.target)}, to ${Ge(e.volume)} over ${e.rampS} s,
          ${r}
        </p>
        ${a?n`<p data-fallback>${a} The alarm rings the bell chime instead.</p>`:u}
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
                </button>`:u}
          <button type="button" data-alarm=${e.id} aria-label="Edit alarm ${e.id}" @click=${this._onEdit}>Edit</button>
          <button type="button" data-alarm=${e.id} aria-label="Delete alarm ${e.id}" @click=${this._onDelete}>
            Delete
          </button>
        </div>
        ${this._refusal(ge(e.id))}
      </li>
    `}_offeredSources(){let e=t=>(this.stored??[]).filter(r=>r.kind===t).map(r=>({value:`stored:${r.id}`,name:r.name}));return[{kind:"chime",label:"Chimes",options:(this.chimes??[]).map(t=>({value:`chime:${t}`,name:t}))},{kind:"line-in",label:"Inputs",options:(this.inputs??[]).map(t=>({value:t.source,name:t.label}))},{kind:"url",label:"Stored stream URLs",options:e("url")},{kind:"spotify",label:"Stored Spotify URIs",options:e("spotify")}]}_alarmDraft(){let e=this._alarm,t=[...this.rooms??[],...this.savedGroups??[]],r=this._offeredSources().flatMap(a=>a.options)[0];return{...e,target:e.target||(t[0]?.id??""),source:e.source||(r?.value??"")}}_setAlarm(e){this._alarm={...this._alarm,...e}}_onAlarmText(e){this._setAlarm({alarm:e.target.value.trim()})}_onAlarmChoice(e){this._setAlarm({[e.target.dataset.field]:e.target.value})}_onAlarmTime(e){if(!Rs(e.target.value)){e.target.value=this._alarm.time;return}this._setAlarm({time:e.target.value})}_onAlarmDay(e){let t=e.currentTarget.dataset.day,r=this._alarm.days.includes(t)?this._alarm.days.filter(a=>a!==t):A.filter(a=>a===t||this._alarm.days.includes(a));this._setAlarm({days:r})}_onAlarmVolume(e){this._setAlarm({volume:Number(e.target.value)})}_onAlarmCount(e){let{field:t,max:r}=e.target.dataset,a=Tr(e.target.value,Number(r));e.target.value=String(a),this._setAlarm({[t]:a})}_onAlarmEnabled(){this._setAlarm({enabled:!this._alarm.enabled})}_onSave(){this._ask(wr,Oe(this._alarmDraft()))}_kindNotes(e){let t=[];this.chimes===null&&t.push(["chime","This server does not say which chimes it has, so none is offered here."]),(this.inputs??[]).length===0&&t.push(["line-in","No input is offered now: no speaker with a line-in is connected."]);let r=new Set((this.stored??[]).map(o=>o.kind));r.has("url")||t.push(["url","No stream URL is stored: add one under Stored sources."]),r.has("spotify")?this.receivers===null&&t.push(["spotify","This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]):t.push(["spotify","No Spotify URI is stored: add one under Stored sources."]);let a=e.source?this._unplayable(e.source,e.target):"",i=this._storedOf(e.source)?.kind==="spotify"?"spotify":"chosen";return a&&!(i==="spotify"&&this.receivers===null)&&t.push([i,`${a} The alarm would ring the bell chime instead.`]),t.map(([o,c])=>n`<p data-unavailable=${o}>${c}</p>`)}_alarmForm(){let e=this._alarmDraft(),t=this.rooms??[],r=this.savedGroups??[],a=[...t,...r],i=this._offeredSources(),o=l=>n`<option value=${l.value}>${l.name}</option>`,c=l=>n`<option value=${l.id}>${l.name}</option>`,d=i.some(l=>l.options.some(h=>h.value===e.source)),p=this._alarmOf(e.alarm)!==null,m=e.alarm!==""&&e.target!==""&&e.source!=="";return n`
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
            ${a.some(l=>l.id===e.target)||!e.target?u:n`<option value=${e.target}>${e.target} (not on this server now)</option>`}
            ${t.length===0?u:n`<optgroup label="Rooms">${t.map(c)}</optgroup>`}
            ${r.length===0?u:n`<optgroup label="Saved groups">${r.map(c)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${e.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${A.map(l=>n`<button
                type="button"
                data-day=${l}
                aria-label="${Ve[l][1]}, the alarm"
                aria-pressed=${e.days.includes(l)?"true":"false"}
                @click=${this._onAlarmDay}
              >
                ${Ve[l][0]}
              </button>`)}
          <p data-value="days">${e.days.length===0?"No day: it rings once, at the next such time.":"It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${e.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${d||!e.source?u:n`<option value=${e.source}>${e.source} (not on this server now)</option>`}
            ${i.map(l=>l.options.length===0?u:n`<optgroup label=${l.label} data-kind=${l.kind}>${l.options.map(o)}</optgroup>`)}
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
            aria-valuetext=${Ge(e.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${Ge(e.volume)}</span>
        </div>
        <div class="row">
          <label for="alarm-ramp">Rises over, seconds</label>
          <input
            id="alarm-ramp"
            type="number"
            min="0"
            max=${te}
            step="1"
            data-field="rampS"
            data-max=${te}
            .value=${String(e.rampS)}
            aria-label="Alarm ramp, seconds"
            @change=${this._onAlarmCount}
          />
          <label for="alarm-duration">Plays for, minutes</label>
          <input
            id="alarm-duration"
            type="number"
            min="0"
            max=${re}
            step="1"
            data-field="durationMin"
            data-max=${re}
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
        ${this._refusal(wr)}
      </div>
    `}_onForget(e){let t=e.currentTarget.dataset.stored;this._ask(Sr(t),zt(t))}_onSourceField(e){this._source={...this._source,[e.target.dataset.field]:e.target.value.trim()}}_onStore(){let{id:e,kind:t,value:r,name:a}=this._source;this._ask(Ar,Rt(e,t,r,a||e))}_storedRow(e){return n`
      <li data-stored=${e.id}>
        <h4>${e.name}</h4>
        <p><span data-value="kind">${ve[e.kind]??e.kind}</span>, <span data-id>${e.id}</span></p>
        <p data-value="value">${e.value}</p>
        <div class="row">
          <button type="button" data-stored=${e.id} aria-label="Forget stored source ${e.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(Sr(e.id))}
      </li>
    `}_storedForm(){let e=this._source,t=e.kind==="spotify";return n`
      <div class="draft" data-draft="stored">
        <div class="row">
          <label for="stored-kind">Kind</label>
          <select id="stored-kind" data-field="kind" data-holds=${e.kind} aria-label="Stored source kind" @change=${this._onSourceField}>
            <option value="url">${ve.url}</option>
            <option value="spotify">${ve.spotify}</option>
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
        ${this._refusal(Ar)}
      </div>
    `}_sleepTargets(){return[...this.rooms??[],...this.formedGroups??[]]}_onCancel(e){let t=e.currentTarget.dataset.target;this._ask(xr(t),Ce(t,0))}_onSleepTarget(e){this._timer={...this._timer,target:e.target.value}}_onSleepMinutes(e){let t=Tr(e.target.value,se);e.target.value=String(t),this._timer={...this._timer,minutes:t}}_onSleep(){let e=this._timer.target||(this._sleepTargets()[0]?.id??"");e&&this._ask(Er,Ce(e,this._timer.minutes))}_sleepRow(e){let t=this._place(e.target),r=e.remainingS===null?`${e.minutes??"?"} min asked for`:zs(this._left(e));return n`
      <li data-sleep=${e.target}>
        <h4>${t}</h4>
        <div class="row">
          <span class="figure" data-value="left">${r}</span>
          <button type="button" data-target=${e.target} aria-label="Cancel sleep timer for ${t}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(xr(e.target))}
      </li>
    `}_sleepForm(){let e=this.rooms??[],t=this.formedGroups??[],r=this._timer.target||(this._sleepTargets()[0]?.id??""),a=i=>n`<option value=${i.id}>${i.name}</option>`;return n`
      <div class="draft" data-draft="sleep">
        <div class="row">
          <label for="sleep-target">For</label>
          <select id="sleep-target" data-holds=${r} aria-label="Sleep timer target" @change=${this._onSleepTarget}>
            ${e.length===0?u:n`<optgroup label="Rooms">${e.map(a)}</optgroup>`}
            ${t.length===0?u:n`<optgroup label="Groups playing now">${t.map(a)}</optgroup>`}
          </select>
          <label for="sleep-minutes">Minutes</label>
          <input
            id="sleep-minutes"
            type="number"
            min="0"
            max=${se}
            step="1"
            .value=${String(this._timer.minutes)}
            aria-label="Sleep timer minutes"
            @change=${this._onSleepMinutes}
          />
          <button type="button" aria-label="Start sleep timer" ?disabled=${!r} @click=${this._onSleep}>Start</button>
        </div>
        <p>It fades the room out and stops it when the time is up. 0 minutes cancels the timer it has.</p>
        ${this._refusal(Er)}
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
    `}};customElements.define("chorus-alarms",Ye);var Xe="alarms",Ke=s=>s.map(({id:e,name:t})=>({id:e,name:t}));w({id:Xe,path:"alarms",title:()=>"Alarms and sleep timers",render:(s,{view:e,refusals:t,refusalFields:r})=>{let a=e.groups??[];return n`
      <chorus-alarms
        .known=${e.state!==null}
        .heard=${e.state}
        .alarms=${pr(e.state)}
        .stored=${mr(e.state)}
        .sleep=${fr(e.state)}
        .chimes=${gr(e.state)}
        .receivers=${vr(e.state)}
        .inputs=${e.inputs??[]}
        .rooms=${Ke(e.rooms)}
        .savedGroups=${Ke(a.filter(i=>i.kind==="saved"))}
        .formedGroups=${Ke(a.filter(i=>i.rooms.length>0))}
        .refusals=${t}
        .refusalFields=${r}
      ></chorus-alarms>
    `}});function Z(s,e){return e.find(t=>t.id===s.group&&t.rooms.some(r=>r.id===s.id))??null}function Or(s,e,t){if(!s||!e)return null;let r=Z(s,t);return e.kind==="alone"?r?ee(s.id):null:typeof e.id!="string"||!e.id?null:e.kind==="group"?r&&r.id===e.id?null:Ee(s.id,e.id):e.kind==="room"?e.id===s.id||r&&r.rooms.some(a=>a.id===e.id)?null:Ee(s.id,e.id):null}var Qe=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function Cr(s){if(s==="alone")return{kind:"alone"};let e=String(s).indexOf(":");if(e<1)return null;let t=s.slice(0,e),r=s.slice(e+1);return(t==="room"||t==="group")&&r?{kind:t,id:r}:null}function Nr(s,e){let t=Z(s,e);return t?Qe({kind:"group",id:t.id}):"alone"}function Rr(s,e,t){return[{value:"alone",label:"Alone"},...t.map(r=>({value:Qe({kind:"group",id:r.id}),label:r.name})),...e.filter(r=>r.id!==s.id&&!Z(r,t)).map(r=>({value:Qe({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var zr=s=>`autoplay:${s}`;function Is(s){let e=hr(s.state),t=o=>e.find(c=>c.input===o)??null,r=(s.inputs??[]).map(o=>({input:o.id,label:o.label,offered:!0,rule:t(o.id)})),a=new Set(r.map(o=>o.input)),i=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(o=>o&&typeof o.input=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.input,o.name]));return[...r,...e.filter(o=>!a.has(o.input)).map(o=>({input:o.input,label:i.get(o.input)??o.input,offered:!1,rule:o}))]}var Ze=class extends g{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={}}_row(e){return(this.rows??[]).find(t=>t.input===e)??null}_ask(e,t,r){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:zr(e.input),body:Ot(e.input,t,r,e.rule??{})},bubbles:!0,composed:!0}))}_onSwitch(e){let t=this._row(e.currentTarget.dataset.input);t?.rule&&this._ask(t,t.rule.target,!t.rule.enabled)}_onTarget(e){let t=this._row(e.target.dataset.input),r=e.target.value,a=t?.rule?.target??"";e.target.value=a,!(!t||!r||r===a)&&this._ask(t,r,t.rule?.enabled??!1)}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-input]")){let t=this._row(e.dataset.input)?.rule?.target??"";e.value!==t&&(e.value=t)}}_targets(e){let t=this.rooms??[],r=this.groups??[],a=!e||[...t,...r].some(o=>o.id===e.target),i=o=>n`<option value=${o.id} ?selected=${e?.target===o.id}>${o.name}</option>`;return n`
      ${e?u:n`<option value="" selected>Nowhere yet</option>`}
      ${a?u:n`<option value=${e.target} selected>${e.target} (not on this server now)</option>`}
      ${t.length===0?u:n`<optgroup label="Rooms">${t.map(i)}</optgroup>`}
      ${r.length===0?u:n`<optgroup label="Saved groups">${r.map(i)}</optgroup>`}
    `}_input(e){let{input:t,label:r,offered:a,rule:i}=e,o=this.refusals?.[zr(t)]??"",c=i?i.enabled?"On":"Off":"Choose where it plays, then switch it on";return n`
      <li data-input=${t}>
        <h3>${r}</h3>
        ${r===t?u:n`<p data-id>${t}</p>`}
        ${a?u:n`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${t}
            aria-label="Autoplay for ${r}"
            aria-pressed=${i?.enabled?"true":"false"}
            ?disabled=${!i}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${c}</span>
        </div>
        <div class="row">
          <label for="target-${t}">Plays in</label>
          <select id="target-${t}" data-input=${t} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(i)}
          </select>
        </div>
        <p role="alert">${o?`Refused: ${o}`:u}</p>
      </li>
    `}render(){return this.rows===null?n`<p role="status" data-missing>Reading this server's inputs.</p>`:n`
      <h2>Autoplay</h2>
      <p>An input with a rule that is on plays in its room or its group when its signal arrives.</p>
      ${this.rows.length===0?n`<p role="status" data-none>This server offers no input now, and has no autoplay rule.</p>`:n`<ul aria-label="Inputs">
            ${this.rows.map(e=>this._input(e))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",Ze);var et="autoplay",Ir=s=>s.map(({id:e,name:t})=>({id:e,name:t}));w({id:et,path:"autoplay",title:()=>"Autoplay",render:(s,{view:e,refusals:t})=>n`
    <chorus-autoplay
      .rows=${e.state===null?null:Is(e)}
      .rooms=${Ir(e.rooms)}
      .groups=${Ir((e.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${t}
    ></chorus-autoplay>
  `});var Mr={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},be=s=>(...e)=>({_$litDirective$:s,values:e}),D=class{constructor(e){}get _$AU(){return this._$AM._$AU}_$AT(e,t,r){this._$Ct=e,this._$AM=t,this._$Ci=r}_$AS(e,t){return this.update(e,t)}update(e,t){return this.render(...t)}};var{I:Ms}=ar,Lr=s=>s;var Pr=()=>document.createComment(""),j=(s,e,t)=>{let r=s._$AA.parentNode,a=e===void 0?s._$AB:e._$AA;if(t===void 0){let i=r.insertBefore(Pr(),a),o=r.insertBefore(Pr(),a);t=new Ms(i,o,s,s.options)}else{let i=t._$AB.nextSibling,o=t._$AM,c=o!==s;if(c){let d;t._$AQ?.(s),t._$AM=s,t._$AP!==void 0&&(d=s._$AU)!==o._$AU&&t._$AP(d)}if(i!==a||c){let d=t._$AA;for(;d!==i;){let p=Lr(d).nextSibling;Lr(r).insertBefore(d,a),d=p}}}return t},C=(s,e,t=s)=>(s._$AI(e,t),s),Ls={},$e=(s,e=Ls)=>s._$AH=e,Fr=s=>s._$AH,_e=s=>{s._$AR(),s._$AA.remove()};var Ur=(s,e,t)=>{let r=new Map;for(let a=e;a<=t;a++)r.set(s[a],a);return r},ye=be(class extends D{constructor(s){if(super(s),s.type!==Mr.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,e,t){let r;t===void 0?t=e:e!==void 0&&(r=e);let a=[],i=[],o=0;for(let c of s)a[o]=r?r(c,o):o,i[o]=t(c,o),o++;return{values:i,keys:a}}render(s,e,t){return this.dt(s,e,t).values}update(s,[e,t,r]){let a=Fr(s),{values:i,keys:o}=this.dt(e,t,r);if(!Array.isArray(a))return this.ut=o,i;let c=this.ut??=[],d=[],p,m,l=0,h=a.length-1,f=0,b=i.length-1;for(;l<=h&&f<=b;)if(a[l]===null)l++;else if(a[h]===null)h--;else if(c[l]===o[f])d[f]=C(a[l],i[f]),l++,f++;else if(c[h]===o[b])d[b]=C(a[h],i[b]),h--,b--;else if(c[l]===o[b])d[b]=C(a[l],i[b]),j(s,d[b+1],a[l]),l++,b--;else if(c[h]===o[f])d[f]=C(a[h],i[f]),j(s,a[l],a[h]),h--,f++;else if(p===void 0&&(p=Ur(o,f,b),m=Ur(c,l,h)),p.has(c[l]))if(p.has(c[h])){let y=m.get(o[f]),v=y!==void 0?a[y]:null;if(v===null){let k=j(s,a[l]);C(k,i[f]),d[f]=k}else d[f]=C(v,i[f]),j(s,a[l],v),a[y]=null;f++}else _e(a[h]),h--;else _e(a[l]),l++;for(;f<=b;){let y=j(s,d[b+1]);C(y,i[f]),d[f++]=y}for(;l<=h;){let y=a[l++];y!==null&&_e(y)}return this.ut=o,$e(s,d),T}});var Dr=be(class extends D{constructor(){super(...arguments),this.key=u}render(s,e){return this.key=s,e}update(s,[e,t]){return e!==this.key&&($e(s),this.key=e),t}});var Ps={playing:"Playing",paused:"Paused",buffering:"Buffering"};function Fs(s,e=[]){if(!s)return"Unavailable";let t=e.find(o=>o.source===s);if(t)return t.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...a]=s.split(":"),i=a.join(":");return r==="line-in"&&i?`Input ${i}`:r==="player"&&i?`Network player ${i}`:r==="chime"&&i?`Chime ${i}`:r==="soloist"&&i?"Spotify":s}var tt=class extends g{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(e){this._failed=e.target.getAttribute("src")}_onInput(e){e.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:wt(this.target,e.source)},bubbles:!0,composed:!0}))}_artwork(e){let t=n`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!e.artwork||e.artwork===this._failed?t:Dr(e.artwork,n`<img
        data-artwork="image"
        src=${e.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let e=this.nowPlaying,t=this.inputs??[];return n`
      ${e?n`<div class="now" data-now-playing=${e.state??"unknown"}>
            ${this._artwork(e)}
            <div class="words">
              <p data-title>${e.title??"Unknown title"}</p>
              ${e.artist?n`<p data-artist>${e.artist}</p>`:u}
              ${e.album?n`<p data-album>${e.album}</p>`:u}
              <p data-state>${Ps[e.state]??"Unavailable"}</p>
            </div>
          </div>`:u}
      <p class="row" data-source=${this.source??""}>Source: ${Fs(this.source,t)}</p>
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
          </ul>`:u}
    `}};customElements.define("chorus-playing",tt);var Us=s=>`${Math.round(s/10)}%`,rt=class extends g{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._slider;if(!t||!this.group||this.group.volume===null)return;let r=e.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(t.value=String(this.group.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(St(this.group.id,Number(e.target.value)))}_onActivate(){this._ask(ee(this.group.id))}_onRemove(e){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:e.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let e=this.group;return e.kind==="live"?"Live group":e.active?"Saved group, active":e.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let e=this.group,t=new Set(e.rooms.map(i=>i.id)),r=e.defined??[],a=new Set(r.map(i=>i.id));return[...r.map(i=>({...i,playing:t.has(i.id)})),...e.rooms.filter(i=>!a.has(i.id)).map(i=>({...i,playing:!0}))]}render(){let e=this.group;if(!e)return u;let t=e.volume===null?"":Us(this._dragged??e.volume);return n`
      <h2>${e.name}</h2>
      <p data-kind=${e.kind} data-active=${e.active===null?u:String(e.active)}>
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
          ></chorus-playing>`:u}
      ${e.kind==="saved"&&!e.active?n`<div class="row">
            <button type="button" aria-label="Group the rooms of ${e.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:u}
      ${e.volume===null?u:n`<div class="row">
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
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:u}</p>
    `}};customElements.define("chorus-group-card",rt);var st=class extends g{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
      ${this.groups!==null&&e.length===0?n`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:u}
      <ul>
        ${ye(e,r=>r.id,r=>n`<li
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
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:u}
      </p>
    `}};customElements.define("chorus-groups",st);var jr=Object.freeze(["phone","desktop"]),Ds=48,js=`(min-width: ${Ds}em)`;function Wr(s,e=globalThis){if(typeof e?.matchMedia!="function")return s("phone"),()=>{};let t=e.matchMedia(js),r=()=>s(t.matches?"desktop":"phone");return t.addEventListener("change",r),r(),()=>t.removeEventListener("change",r)}var at=s=>`limits:${s}`,Hr={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},Ws=Object.freeze({days:A,start:"22:00",end:"07:00",limit:250}),P=s=>`${Math.round(s/10)}%`,Br=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),Hs=({days:s,start:e,end:t,limit:r})=>({days:s,start:e,end:t,limit:r}),it=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...Ws},this._held=new Set,this._asked=null,this._unanswered=0}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||t)&&(r.value=r.dataset.server)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:at(this.room.id),body:e,done:t},bubbles:!0,composed:!0}))}_askWindows(e){let t=this.room.id,r=(this._asked?.room===t?this._asked.windows:this.room.limits.windows).map(Hs);e(r),this._asked={room:t,windows:r},this._unanswered+=1,this._ask(Et(t,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onFocus(e){this._held.add(e.target.dataset.key)}_onBlur(e){let{key:t,server:r}=e.target.dataset;this._held.delete(t),this._release(t),e.target.value=r}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.key]:Number(e.target.value)}}_onLimitChange(e){this._release(e.target.dataset.key),this._ask(xt(this.room.id,Number(e.target.value)))}_onEnabled(){this._ask(Tt(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(e){let t=Number(e.target.dataset.window),r=Number(e.target.value);this._release(e.target.dataset.key),this._askWindows(a=>{a[t]={...a[t],limit:r}})}_onWindowTime(e){let{window:t,edge:r,server:a}=e.target.dataset,i=e.target.value;if(!Br(i)){e.target.value=a;return}i!==a&&this._askWindows(o=>{o[Number(t)]={...o[Number(t)],[r]:i}})}_onWindowDay(e){let{window:t,day:r}=e.currentTarget.dataset;this._askWindows(a=>{let i=a[Number(t)],o=i.days.includes(r)?i.days.filter(c=>c!==r):[...i.days,r];a[Number(t)]={...i,days:o}})}_onRemove(e){let t=Number(e.currentTarget.dataset.window);this._askWindows(r=>r.splice(t,1))}_onDraftDay(e){let t=e.currentTarget.dataset.day,r=this._draft.days.includes(t)?this._draft.days.filter(a=>a!==t):A.filter(a=>a===t||this._draft.days.includes(a));this._draft={...this._draft,days:r}}_onDraftTime(e){let t=e.target.dataset.edge;if(!Br(e.target.value)){e.target.value=this._draft[t];return}this._draft={...this._draft,[t]:e.target.value}}_onDraftLimit(e){this._draft={...this._draft,limit:Number(e.target.value)}}_onAdd(){this._askWindows(e=>e.push({...this._draft}))}_days(e,t,r,a){let i=this.room;return n`
      <div class="row" role="group" aria-label="Days of ${t} for ${i.name}">
        ${A.map(o=>n`<button
              type="button"
              data-day=${o}
              data-window=${a??u}
              aria-label="${Hr[o][1]}, ${t} for ${i.name}"
              aria-pressed=${e.includes(o)?"true":"false"}
              @click=${r}
            >
              ${Hr[o][0]}
            </button>`)}
      </div>
    `}_window(e,t,r){let a=this.room,i=`window ${t+1}`,o=a.limits.quietEnabled!==!1,c=e.active?o?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return n`<li data-window=${t}><p data-value="active">Unavailable</p></li>`;let d=`window-${t}`;return n`
      <li data-window=${t} ?data-active=${e.active}>
        <div class="row">
          <strong>Window ${t+1}</strong>
          <span data-value="active" ?data-active=${e.active&&o}>${c}</span>
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
            aria-valuetext=${P(this._dragged[`${d}-limit`]??e.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${P(this._dragged[`${d}-limit`]??e.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${t} aria-label="Remove ${i} for ${a.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(e){let t=this.room;if(e>=Te)return n`<p data-full>A room has at most ${Te} windows. Remove one to add another.</p>`;let r=this._draft,a="the new window";return n`
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
            aria-valuetext=${P(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${P(r.limit)}</span>
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
      </p>`;let{limit:t,effectiveLimit:r,quietEnabled:a,windows:i}=e.limits,o=i.every(p=>p.start&&p.end&&p.limit!==null&&p.days.length>0),c=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:u,d=t===null?"Unavailable":P(this._dragged.limit??t);return n`
      <h2>Volume limits of ${e.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${t===null?u:n`<input
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
        <span class="figure" data-value="effective">${r===null?"Unavailable":P(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${e.volume===null?"Unavailable":P(e.volume)}</span>
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
      ${i.length===0?n`<p data-none>This room has no quiet-hours window.</p>`:u}
      <ol aria-label="Quiet-hours windows of ${e.name}">
        ${i.map((p,m)=>this._window(p,m,o))}
      </ol>
      ${o?n`<h3>Add a window</h3>
            ${this._adding(i.length)}`:n`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||u}>${c}</p>
    `}};customElements.define("chorus-room-limits",it);var ot="room-limits";w({id:ot,path:"rooms/:room/limits",title:({room:s},e)=>`Volume limits of ${U(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-limits
      .room=${U(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[at(s)]??""}
      .refusalField=${r[at(s)]??""}
    ></chorus-room-limits>
  `});var qr=Object.freeze(["app","kiosk"]),nt="chorus.kiosk",Jr="1";function Bs(s){let e=new URLSearchParams(s).get("kiosk");return e===null?null:e==="0"||e==="false"?"app":"kiosk"}function Vr(s,e){let t=Bs(s);try{if(t==="kiosk")e?.setItem(nt,Jr);else if(t==="app")e?.removeItem(nt);else return e?.getItem(nt)===Jr?"kiosk":"app"}catch{}return t??"app"}function Gr(s=globalThis){try{return s.localStorage??null}catch{return null}}var lt=s=>`sound:${s}`,Kr=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],Js=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],qs=s=>`${s>0?"+":""}${s} dB`,dt=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(e){return this.renderRoot.querySelector(`input[data-field="${e}"]`)}updated(e){if(!this.room)return;let t=e.has("refusal")&&!!this.refusal;t&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of Kr){let a=this._slider(r),i=this.room.sound[r];!a||i===null||(!this._held.has(r)||t)&&(a.value=String(i))}}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:lt(this.room.id),body:At(this.room.id,{[e]:t})},bubbles:!0,composed:!0}))}_release(e){if(!(e in this._dragged))return;let{[e]:t,...r}=this._dragged;this._dragged=r}_onSliderFocus(e){this._held.add(e.target.dataset.field)}_onSliderBlur(e){let t=e.target.dataset.field;this._held.delete(t),this._release(t);let r=this.room?.sound[t];r!=null&&(e.target.value=String(r))}_onSliderInput(e){this._dragged={...this._dragged,[e.target.dataset.field]:Number(e.target.value)}}_onSliderChange(e){let t=e.target.dataset.field;this._release(t),this._ask(t,Number(e.target.value))}_onSwitch(e){let t=e.currentTarget.dataset.field;this._ask(t,!this.room.sound[t])}_tone({field:e,name:t}){let r=this.room,a=r.sound[e],i=a===null?"Unavailable":qs(this._dragged[e]??a);return n`
      <div class="row">
        <label for=${e}>${t}</label>
        ${a===null?u:n`<input
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
      </p>`;let t=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:u;return n`
      <h2>Sound of ${e.name}</h2>
      ${Kr.map(r=>this._tone(r))} ${Js.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||u}>${t}</p>
    `}};customElements.define("chorus-room-sound",dt);var ut="room-sound";w({id:ut,path:"rooms/:room/sound",title:({room:s},e)=>`Sound of ${U(e.rooms,s)?.name??s}`,render:({room:s},{view:e,refusals:t,refusalFields:r})=>n`
    <chorus-room-sound
      .room=${U(e.rooms,s)}
      .roomId=${s}
      .known=${e.state!==null}
      .refusal=${t[lt(s)]??""}
      .refusalField=${r[lt(s)]??""}
    ></chorus-room-sound>
  `});var Vs={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Gs=s=>`${Math.round(s/10)}%`,ct=class extends g{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(e){let t=this._list;t&&(t.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let a=e.has("refusal")&&!!this.refusal;a&&(this._dragged=null),(!this._sliderHeld||a)&&(r.value=String(this.room.volume))}_ask(e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:e},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(e){this._dragged=Number(e.target.value)}_onSliderChange(e){this._dragged=null,this._ask(yt(this.room.id,Number(e.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(e){let t=e.target.value;if(e.target.value=this.place,t===this.place)return;let r=Cr(t);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(kt(this.room.id,!this.room.muted))}render(){let e=this.room;if(!e)return u;let t=e.volume===null?"Unavailable":Gs(this._dragged??e.volume);return n`
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
        <a href=${S(ut,{room:e.id})} data-route aria-label="Sound for ${e.name}">Sound</a>
        <a href=${S(ot,{room:e.id})} data-route aria-label="Limits for ${e.name}">Limits</a>
      </div>
      ${e.bond.length===0?u:n`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${e.bond.map(r=>n`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${Vs[r.role]??r.role}: ${r.name}
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
          ></chorus-playing>`:u}
      <div class="row">
        <label for="volume">Volume</label>
        ${e.volume===null?u:n`<input
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
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:u}</p>
    `}};customElements.define("chorus-room-card",ct);var ht=class extends g{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
          </p>`:u}
      <ul>
        ${ye(e??[],a=>a.id,a=>n`<li
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
                .places=${Rr(a,e,t)}
                .place=${Nr(a,t)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",ht);var pt="chorus-setup-",ft=6,Ks=12,Ys=Object.freeze({form:"GET /",takes:"POST /join",title:"chorus speaker setup"}),Xs=Object.freeze([{id:"power",title:"Switch the speaker on",text:[`A Wi-Fi speaker that knows no network raises a Wi-Fi access point of its own, named ${pt} and ${ft} characters (${pt}<${ft} characters>).`,`Its setup secret is ${Ks} characters and is the access point's password. The speaker prints it, and the address of its join page, on its serial console when the access point comes up.`]},{id:"access-point",title:"Join the speaker's access point",text:[`In this phone's Wi-Fi settings, join the network ${pt}<${ft} characters> with the setup secret as its password. Accept that it has no internet.`,"The phone is then off the house's network, and this page cannot reach the chorus server until it is back. That is expected. Leave this page open."]},{id:"join-page",title:"Open the speaker's join page",text:[`In the phone's browser, open the address the speaker printed (http://<address>/). The speaker serves the page itself, on its access point: it is titled "${Ys.title}" and is a form with two fields.`,"Type the house network's name and its passphrase into that page, and press Join. They go to the speaker and nowhere else: this app never asks for them. The network has to be on 2.4 GHz and have a passphrase; the speaker refuses an open network.",'The page answers "Received". If the join fails the access point stays up: join it again and load the page again, and it says why above the form (auth-error for a wrong passphrase, network-not-found for a name it cannot see).']},{id:"return",title:"Come back to the house's network",text:["The speaker takes its access point down and joins the house's network. The phone goes back to the house's network on its own, or join it again in the Wi-Fi settings.","When the speaker reaches the chorus server it is adopted, and this page says so by itself. There is nothing to press."]}]);function Qs(s,e){let t=new Set(s??[]);return(e??[]).filter(r=>!t.has(r.id))}var gt="chorus-speaker-setup";function Zs(s){try{let e=JSON.parse(s?.getItem(gt)??"null");return Array.isArray(e)&&e.every(t=>typeof t=="string")?e:null}catch{return null}}function mt(s,e){try{e===null?s?.removeItem(gt):s?.setItem(gt,JSON.stringify(e))}catch{}}var ea=()=>{try{return globalThis.sessionStorage??null}catch{return null}},vt=class extends g{static properties={speakers:{attribute:!1},status:{type:String},back:{type:String},storage:{attribute:!1},_baseline:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.status="connecting",this.back="#/",this.storage=ea(),this._baseline=null}willUpdate(){!this.isConnected||this._baseline!==null||!Array.isArray(this.speakers)||(this._baseline=Zs(this.storage)??this.speakers.map(e=>e.id),mt(this.storage,this._baseline))}disconnectedCallback(){super.disconnectedCallback(),mt(this.storage,null),this._baseline=null}_onAgain(){Array.isArray(this.speakers)&&(this._baseline=this.speakers.map(e=>e.id),mt(this.storage,this._baseline))}_status(e){return e.length>0?u:this.status==="lost"||this.status==="signed-out"?n`<p role="status" data-away>
        ${this.status==="signed-out"?"Signed out of the chorus server: sign in again to go on.":"This page cannot reach the chorus server now. That is expected while the phone is on the speaker's access point: it goes on by itself when the phone is back on the house's network."}
      </p>`:this._baseline===null?n`<p role="status" data-waiting>Reading this server's speakers.</p>`:n`<p role="status" data-waiting>
      Waiting for a new speaker. This page goes on by itself when one is adopted.
    </p>`}_done(e){return e.length===0?u:n`
      <div data-done role="status">
        ${e.map(t=>n`<p data-arrived=${t.id}>${t.name} (${t.id}) joined and was adopted.</p>`)}
        <p>It has no name of its own and is in no room yet.</p>
        <div class="row">
          <a href=${this.back} data-route aria-label="Name the new speaker and give it a room">Name it and give it a room</a>
          <button type="button" aria-label="Set up another speaker" @click=${this._onAgain}>Set up another</button>
        </div>
      </div>
    `}render(){let e=this._baseline===null?[]:Qs(this._baseline,this.speakers);return n`
      <h2>Set up a Wi-Fi speaker</h2>
      <p>
        A compact Wi-Fi speaker learns the house's network from a phone, on a page the speaker serves itself. This app
        says the steps and watches for the speaker; it never asks for the network's passphrase.
      </p>
      ${this._done(e)}
      <ol aria-label="Steps" ?data-complete=${e.length>0}>
        ${Xs.map((t,r)=>n`
            <li data-step=${t.id}>
              <h3>${r+1}. ${t.title}</h3>
              ${t.text.map(a=>n`<p>${a}</p>`)}
            </li>
          `)}
      </ol>
      ${this._status(e)}
    `}};customElements.define("chorus-speaker-setup",vt);var Yr=s=>`speaker:${s}`,Xr="firmware",ta={wired:"Wired",wireless:"Wi-Fi"},Qr=["requested","receiving","verified","pending_verify"],ra=["requested","receiving"];function sa(s){let e=s.imageVersion?`version ${s.imageVersion}`:"";return s.image?e?`image ${s.image} (${e})`:`image ${s.image}`:e||"an image"}function aa(s){let e=sa(s),t=s.version?`version ${s.version}`:"the version it ran before";switch(s.state){case"idle":return"No install is in progress.";case"requested":return`Install requested: the server is offering ${e} to the speaker.`;case"receiving":return`Receiving ${e}: ${s.received} of ${s.size} bytes.`;case"verified":return`Written and checked: ${e}. The speaker restarts into it.`;case"pending_verify":return`On trial: the speaker runs ${t} and has not confirmed it yet.`;case"confirmed":return`Installed: ${e} confirmed itself, and the speaker runs ${t}.`;case"rolled_back":return`Rolled back: ${e} did not confirm, and the speaker runs ${t} again. Nothing retries it.`;case"refused":return`Refused by the speaker: ${e} was not installed.`;case"interrupted":return`Interrupted: the install of ${e} did not finish and is not resumed. Install again to start over.`;case"cancelled":return`Cancelled: the install of ${e} was abandoned.`;default:return`Firmware state: ${s.state}.`}}var bt=class extends g{static properties={speakers:{attribute:!1},keyChanges:{attribute:!1},rooms:{attribute:!1},images:{attribute:!1},refusals:{attribute:!1},setup:{type:String},_drafts:{state:!0},_forgetting:{state:!0},_installing:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.keyChanges=[],this.rooms=[],this.refusals={},this.setup="",this.images=null,this._drafts={},this._forgetting=null,this._installing=null}_speaker(e){return(this.speakers??[]).find(t=>t.id===e)??null}willUpdate(e){if(this._installing!==null&&(e.has("speakers")||e.has("images"))){let r=this._speaker(this._installing.speaker);r&&this._offers(r).some(i=>i.name===this._installing.image)||(this._installing=null)}if(!e.has("speakers"))return;let t=Object.entries(this._drafts).filter(([r,a])=>{let i=this._speaker(r);return i&&!(i.named&&i.name===a.trim())});t.length!==Object.keys(this._drafts).length&&(this._drafts=Object.fromEntries(t)),this._forgetting!==null&&!this._speaker(this._forgetting)&&(this._forgetting=null)}_ask(e,t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Yr(e),body:t},bubbles:!0,composed:!0}))}_onDraft(e){this._drafts={...this._drafts,[e.target.dataset.speaker]:e.target.value}}_name(e){let t=this._speaker(e),r=(this._drafts[e]??t?.name??"").trim();!t||!r||t.named&&r===t.name||this._ask(e,It(e,r))}_onName(e){this._name(e.currentTarget.dataset.speaker)}_onNameKey(e){e.key==="Enter"&&(e.preventDefault(),this._name(e.target.dataset.speaker))}_onRoom(e){let t=this._speaker(e.target.dataset.speaker),r=e.target.value,a=t?.room??"";e.target.value=a,!(!t||r===a)&&this._ask(t.id,Mt(t.id,r||null))}updated(){for(let e of this.renderRoot.querySelectorAll("select[data-speaker]")){let t=this._speaker(e.dataset.speaker)?.room??"";e.value!==t&&(e.value=t)}}_onForget(e){let{speaker:t,forget:r}=e.currentTarget.dataset;if(r==="ask"){this._forgetting=t;return}this._forgetting=null,r==="yes"&&this._ask(t,Lt(t))}_offers(e){let t=e.firmware;return!t||this.images===null||Qr.includes(t.state)?[]:$r(t,this.images)}_onInstall(e){let{speaker:t,image:r,install:a}=e.currentTarget.dataset;if(a==="ask"){this._installing={speaker:t,image:r};return}let i=this._installing;this._installing=null,!(a!=="yes"||!i||i.speaker!==t||i.image!==r)&&this._ask(t,Pt(t,r))}_onCancelInstall(e){let{speaker:t}=e.currentTarget.dataset;this._ask(t,Ft(t))}_onRescan(){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Xr,body:Ut()},bubbles:!0,composed:!0}))}_rooms(e){let t=this.rooms??[],r=e.room===null||t.some(a=>a.id===e.room);return n`
      <option value="" ?selected=${e.room===null}>No room</option>
      ${r?u:n`<option value=${e.room} selected>${e.room} (not on this server now)</option>`}
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
      `}_firmware(e){let t=e.firmware;if(!t)return u;let{id:r,name:a}=e,i=this.images!==null,o=this._offers(e),c=t.state==="receiving"&&t.size>0;return n`
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
          ${aa(t)}
          ${t.reason?n`<span data-firmware-reason>Reason: ${t.reason}.</span>`:u}
        </p>
        ${c?n`<div class="row">
              <progress
                max=${t.size}
                value=${Math.min(t.received,t.size)}
                aria-label="Install progress of ${a}"
              ></progress>
            </div>`:u}
        ${i&&ra.includes(t.state)?n`<div class="row">
              <button
                type="button"
                data-speaker=${r}
                data-cancel-install
                aria-label="Cancel the install on ${a}"
                @click=${this._onCancelInstall}
              >
                Cancel install
              </button>
            </div>`:u}
        ${i&&t.updateAvailable?n`
              <p data-update-available>Update available</p>
              ${o.map(d=>this._offer(e,d))}
              ${o.length>0&&!e.present?n`<p data-update-absent>The speaker is not connected: it can be installed when it is.</p>`:u}
              ${o.length===0&&!Qr.includes(t.state)?n`<p data-update-unlisted>
                    The server lists no verified image for this board with another version. Rescan the staged images.
                  </p>`:u}
            `:u}
      </section>
    `}_images(){if(this.images===null)return u;let e=this.refusals?.[Xr]??"";return n`
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
        <p role="alert">${e?`Refused: ${e}`:u}</p>
      </section>
    `}_row(e){let{id:t,name:r}=e,a=this.refusals?.[Yr(t)]??"",i=(this.keyChanges??[]).some(d=>d.id===t),o=this._drafts[t],c=(o??r).trim();return n`
      <li data-speaker=${t} ?data-new=${e.isNew}>
        <h3>${r}</h3>
        ${e.isNew?n`<p data-new-mark>New: adopted, not named and in no room yet.</p>`:u}
        ${i?n`<p data-key-changed>A session under this id offered another key and was refused (above).</p>`:u}
        <p data-id>${t}</p>
        <dl>
          <div>
            <dt>Now</dt>
            <dd data-value="present">${e.present?"Connected":"Not connected"}</dd>
          </div>
          <div>
            <dt>Link</dt>
            <dd data-value="link">${ta[e.link]??"Not reported"}</dd>
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
            ?disabled=${!c||e.named&&c===r}
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
        <p role="alert">${a?`Refused: ${a}`:u}</p>
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
      ${e.length===0?u:n`<ul aria-label="Changed keys">
            ${e.map(t=>this._keyChange(t))}
          </ul>`}
      ${this.speakers.length===0?n`<p role="status" data-none>This server has adopted no speaker yet.</p>`:n`<ul aria-label="Adopted speakers">
            ${this.speakers.map(t=>this._row(t))}
          </ul>`}
      ${this._images()}
      ${this.setup?n`<a href=${this.setup} data-route aria-label="Set up a Wi-Fi speaker">Set up a Wi-Fi speaker</a>`:u}
    `}};customElements.define("chorus-speakers",bt);var ke="speakers",Zr="speaker-setup";w({id:ke,path:"speakers",title:()=>"Speakers",render:(s,{view:e,refusals:t})=>n`
    <chorus-speakers
      .speakers=${e.state===null?null:qe(e.state)}
      .keyChanges=${_r(e.state)}
      .rooms=${e.rooms.map(({id:r,name:a})=>({id:r,name:a}))}
      .images=${br(e.state)}
      .refusals=${t}
      .setup=${S(Zr)}
    ></chorus-speakers>
  `});w({id:Zr,path:"speakers/setup",title:()=>"Set up a Wi-Fi speaker",render:(s,{view:e})=>n`
    <chorus-speaker-setup
      .speakers=${e.state===null?null:qe(e.state)}
      .status=${e.status}
      .back=${S(ke)}
    ></chorus-speaker-setup>
  `});var $t=class extends g{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=ur(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",e=>this._onLink(e)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=nr({onStart:e=>{let t=this._room(e);t&&(this._moving={id:e,name:t.name,grouped:!!Z(t,this._groups)})},onOver:e=>{let t=this._over;t?.kind===e?.kind&&t?.id===e?.id||(this._over=e)},onEnd:(e,t)=>{this._moving=null,this._over=null,t&&this._move(e,t)}})}get _groups(){return this._view.groups??[]}_room(e){return this._view.rooms.find(t=>t.id===e)??null}willUpdate(e){qr.includes(this.mode)||(this.mode="app"),jr.includes(this.layout)||(this.layout="phone"),e.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=Wr(e=>{this.layout=e}),this._unroute?.(),this._unroute=this._navigation.watch(e=>{e.address!==this._route.address&&(this._route=e)})}updated(e){if(!e.has("_route")||e.get("_route")===void 0)return;let t=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(t==="groups"?"section":"main");r&&(t&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!t}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(e=>{this._view=e}))}async _send(e,t){if(!this.store)return;this._refusals={...this._refusals,[e]:""},this._refusalFields={...this._refusalFields,[e]:""};let r=await this.store.command(t);r.ok||(this._refusals={...this._refusals,[e]:r.refusal},this._refusalFields={...this._refusalFields,[e]:r.field??""})}_onCommand(e){let{subject:t,room:r,body:a,done:i}=e.detail;this._send(t??r,a).then(()=>i?.())}_move(e,t){let r=this._room(e),a=Or(r,t,this._groups);a&&this._send(e,a)}_onMove(e){this._move(e.detail.room,e.detail.destination)}_onPointerDown(e){this._drag.begin(e)}_onGo(e){let t=e.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=t,this._navigation.back();return}let r=this.renderRoot.querySelector(t==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(e){if(e.defaultPrevented||e.button>0||e.metaKey||e.ctrlKey||e.shiftKey||e.altKey)return;let t=e.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));t&&(e.preventDefault(),t.dataset.route==="back"?this._navigation.back():this._navigation.open(t.getAttribute("href")))}_screen(e){let t=We(e.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return n`
      <main
        aria-label=${t.title(e.params,this._view)}
        data-screen=${t.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${me} data-route="back" aria-label="Back to rooms">Back</a>
        ${t.render(e.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?u:n`
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
      ${this._signedOut()} ${this._route.screen===fe.screen?this._home():this._screen(this._route)}
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
          ${this._moving?`Moving ${this._moving.name}. Drop it on a room or a group.`:u}
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
        <a class="more" href=${S(et)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${S(Xe)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <a class="more" href=${S(ke)} data-route aria-label="Speakers and their setup">Speakers</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",$t);var ia="sw.js";async function es(s=globalThis.navigator){let e=s?.serviceWorker;if(!e||typeof e.register!="function")return null;try{return await e.register(ia,{scope:"./",updateViaCache:"none"})}catch{return null}}function ts({navigator:s=globalThis.navigator,document:e=globalThis.document}={}){let t=null;try{t=s?.wakeLock??null}catch{t=null}if(!t||typeof t.request!="function"||typeof e?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,a=null,i=!1,o=async d=>{try{await d.release()}catch{}},c=()=>{i||r||a||e.visibilityState!=="visible"||(a=(async()=>{try{let d=await t.request("screen");if(i){await o(d);return}r=d,d.addEventListener?.("release",()=>{r===d&&(r=null)})}catch{}finally{a=null}})())};return e.addEventListener("visibilitychange",c),c(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;a;)await a},stop:async()=>{for(i=!0,e.removeEventListener("visibilitychange",c);a;)await a;let d=r;r=null,d&&await o(d)}}}var we=document.querySelector("chorus-app");if(we){we.mode=Vr(window.location.search,Gr(window)),we.mode==="kiosk"&&ts();let s=kr(Dt());we.store=s,s.start()}es();
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
