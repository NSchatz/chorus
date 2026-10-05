function q(s){let t=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function _e(s,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${q(t)}}`}function ye(s,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${t?"true":"false"}}`}function Et(s,t){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(t)}}`}function tt(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function ke(s,t){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(t)}}`}function we(s,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${q(t)}}`}var J=Object.freeze({min:-10,max:10}),Yr=["bass","treble"],Xr=["loudness","night","speech"];function Se(s,t={}){let e=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of Yr){if(t[r]===void 0)continue;let a=Math.min(J.max,Math.max(J.min,Math.round(Number(t[r])||0)));e+=`,"${r}":${a}`}for(let r of Xr)t[r]!==void 0&&(e+=`,"${r}":${t[r]?"true":"false"}`);return`${e}}`}function Ae(s,t){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${q(t)}}`}var A=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),Tt=8;function xe(s,t=[]){let e=t.map(r=>{let a=A.filter(i=>(r.days??[]).includes(i));return`{"days":${JSON.stringify(a)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${q(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${e.join(",")}]}`}function Ee(s,t){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${t?"true":"false"}}`}function Te(s,t,e,{stopOnStandby:r=!0,lowLatency:a=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(t)},"enabled":${e?"true":"false"}${r===!1?',"stop_on_standby":false':""}${a===!1?',"low_latency":false':""}}`}var et=600,rt=720,st=720,xt=(s,t)=>Math.min(t,Math.max(0,Math.round(Number(s)||0)));function Ot({alarm:s,target:t,time:e,days:r=[],source:a,volume:i,rampS:o,durationMin:c,enabled:d}){let p=A.filter(f=>r.includes(f));return`{"v":2,"t":"alarm_set","alarm":${JSON.stringify(s)},"target":${JSON.stringify(t)},"time":${JSON.stringify(String(e))},"days":${JSON.stringify(p)},"source":${JSON.stringify(a)},"volume":${q(i)},"ramp_s":${xt(o,et)},"duration_min":${xt(c,rt)},"enabled":${d?"true":"false"}}`}function Oe(s){return`{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(s)}}`}function Ce(s){return`{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(s)}}`}function Ct(s,t){return`{"v":2,"t":"sleep","target":${JSON.stringify(s)},"minutes":${xt(t,st)}}`}function Ne(s,t,e,r){return`{"v":2,"t":"source_store","id":${JSON.stringify(s)},"kind":${JSON.stringify(t)},"value":${JSON.stringify(e)},"name":${JSON.stringify(r)}}`}function Re(s){return`{"v":2,"t":"source_forget","id":${JSON.stringify(s)}}`}function ze(s,t){return`{"v":2,"t":"speaker_name","speaker":${JSON.stringify(s)},"name":${JSON.stringify(t)}}`}function Me(s,t){return`{"v":2,"t":"speaker_room","speaker":${JSON.stringify(s)},"room":${typeof t=="string"&&t?JSON.stringify(t):"null"}}`}function Pe(s){return`{"v":2,"t":"speaker_forget","speaker":${JSON.stringify(s)}}`}function Nt(s,t,e=""){let r=5381;for(let a of String(e))r=(Math.imul(r,33)^a.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(t)}${e?`#${r.toString(36)}`:""}`}function At(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var $e="Signed out";async function Qr(s){let t="";try{t=(await s.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail){let r=typeof e.field=="string"&&e.field?{field:e.field}:{};return{refusal:e.detail,...r}}}catch{}return{refusal:t||`the server answered ${s.status}`}}var Zr={set:(s,t)=>globalThis.setTimeout(s,t),clear:s=>globalThis.clearTimeout(s)};function Le({fetch:s=globalThis.fetch.bind(globalThis),base:t="../",timers:e=Zr}={}){async function r(){let c=await s(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(At(c))throw Object.assign(new Error($e),{signedOut:!0});if(!c.ok)throw new Error(`the server answered ${c.status}`);return c.json()}async function a(c){let d;try{d=await s(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:c,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(At(d))return{ok:!1,refusal:$e,signedOut:!0};if(!d.ok)return{ok:!1,...await Qr(d)};try{return{ok:!0,state:await d.json()}}catch{return{ok:!0,state:null}}}function i({onState:c,onStatus:d=()=>{}}){let p=!1,f=null,n=null,h=null,m=()=>{h!==null&&e.clear(h),h=null},b=()=>{m(),h=e.set(()=>f?.abort(),4e4)},_=y=>{let O=y.split(`
`).filter(W=>W.startsWith("data:")).map(W=>W.slice(5).replace(/^ /,"")).join(`
`);if(!O)return;let H;try{H=JSON.parse(O)}catch{return}d("live"),c(H)};async function v(){f=new AbortController,b();let y=!1;try{let O=await s(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:f.signal});if(y=At(O),!O.ok||!O.body)throw new Error(`the server answered ${O.status}`);let H=O.body.getReader();f.signal.addEventListener("abort",()=>H.cancel().catch(()=>{}));let W=new TextDecoder,B="";for(;;){let{done:Gr,value:Kr}=await H.read();if(Gr||p||f.signal.aborted)break;b(),B+=W.decode(Kr,{stream:!0}).replace(/\r\n?/g,`
`);let St;for(;(St=B.indexOf(`

`))!==-1;)_(B.slice(0,St)),B=B.slice(St+2)}}catch{}m(),!p&&(d(y?"signed-out":"lost"),n=e.set(()=>{n=null,v()},1e3))}return v(),()=>{p=!0,m(),n!==null&&e.clear(n),f?.abort()}}return{state:r,command:a,events:i,artwork:(c,d)=>Nt(t,c,d)}}var at=globalThis,it=at.ShadowRoot&&(at.ShadyCSS===void 0||at.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,Rt=Symbol(),Ie=new WeakMap,V=class{constructor(t,e,r){if(this._$cssResult$=!0,r!==Rt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(it&&t===void 0){let r=e!==void 0&&e.length===1;r&&(t=Ie.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),r&&Ie.set(e,t))}return t}toString(){return this.cssText}},Ue=s=>new V(typeof s=="string"?s:s+"",void 0,Rt),$=(s,...t)=>{let e=s.length===1?s[0]:t.reduce((r,a,i)=>r+(o=>{if(o._$cssResult$===!0)return o.cssText;if(typeof o=="number")return o;throw Error("Value passed to 'css' function must be a 'css' function result: "+o+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(a)+s[i+1],s[0]);return new V(e,s,Rt)},Fe=(s,t)=>{if(it)s.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let r=document.createElement("style"),a=at.litNonce;a!==void 0&&r.setAttribute("nonce",a),r.textContent=e.cssText,s.appendChild(r)}},zt=it?s=>s:s=>s instanceof CSSStyleSheet?(t=>{let e="";for(let r of t.cssRules)e+=r.cssText;return Ue(e)})(s):s;var{is:ts,defineProperty:es,getOwnPropertyDescriptor:rs,getOwnPropertyNames:ss,getOwnPropertySymbols:as,getPrototypeOf:is}=Object,ot=globalThis,De=ot.trustedTypes,os=De?De.emptyScript:"",ns=ot.reactiveElementPolyfillSupport,G=(s,t)=>s,Mt={toAttribute(s,t){switch(t){case Boolean:s=s?os:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,t){let e=s;switch(t){case Boolean:e=s!==null;break;case Number:e=s===null?null:Number(s);break;case Object:case Array:try{e=JSON.parse(s)}catch{e=null}}return e}},He=(s,t)=>!ts(s,t),je={attribute:!0,type:String,converter:Mt,reflect:!1,useDefault:!1,hasChanged:He};Symbol.metadata??=Symbol("metadata"),ot.litPropertyMetadata??=new WeakMap;var x=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=je){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let r=Symbol(),a=this.getPropertyDescriptor(t,r,e);a!==void 0&&es(this.prototype,t,a)}}static getPropertyDescriptor(t,e,r){let{get:a,set:i}=rs(this.prototype,t)??{get(){return this[e]},set(o){this[e]=o}};return{get:a,set(o){let c=a?.call(this);i?.call(this,o),this.requestUpdate(t,c,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??je}static _$Ei(){if(this.hasOwnProperty(G("elementProperties")))return;let t=is(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(G("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(G("properties"))){let e=this.properties,r=[...ss(e),...as(e)];for(let a of r)this.createProperty(a,e[a])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[r,a]of e)this.elementProperties.set(r,a)}this._$Eh=new Map;for(let[e,r]of this.elementProperties){let a=this._$Eu(e,r);a!==void 0&&this._$Eh.set(a,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let r=new Set(t.flat(1/0).reverse());for(let a of r)e.unshift(zt(a))}else t!==void 0&&e.push(zt(t));return e}static _$Eu(t,e){let r=e.attribute;return r===!1?void 0:typeof r=="string"?r:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let r of e.keys())this.hasOwnProperty(r)&&(t.set(r,this[r]),delete this[r]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Fe(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,r){this._$AK(t,r)}_$ET(t,e){let r=this.constructor.elementProperties.get(t),a=this.constructor._$Eu(t,r);if(a!==void 0&&r.reflect===!0){let i=(r.converter?.toAttribute!==void 0?r.converter:Mt).toAttribute(e,r.type);this._$Em=t,i==null?this.removeAttribute(a):this.setAttribute(a,i),this._$Em=null}}_$AK(t,e){let r=this.constructor,a=r._$Eh.get(t);if(a!==void 0&&this._$Em!==a){let i=r.getPropertyOptions(a),o=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:Mt;this._$Em=a;let c=o.fromAttribute(e,i.type);this[a]=c??this._$Ej?.get(a)??c,this._$Em=null}}requestUpdate(t,e,r,a=!1,i){if(t!==void 0){let o=this.constructor;if(a===!1&&(i=this[t]),r??=o.getPropertyOptions(t),!((r.hasChanged??He)(i,e)||r.useDefault&&r.reflect&&i===this._$Ej?.get(t)&&!this.hasAttribute(o._$Eu(t,r))))return;this.C(t,e,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:r,reflect:a,wrapped:i},o){r&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,o??e??this[t]),i!==!0||o!==void 0)||(this._$AL.has(t)||(this.hasUpdated||r||(e=void 0),this._$AL.set(t,e)),a===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[a,i]of this._$Ep)this[a]=i;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[a,i]of r){let{wrapped:o}=i,c=this[a];o!==!0||this._$AL.has(a)||c===void 0||this.C(a,void 0,i,c)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(e)):this._$EM()}catch(r){throw t=!1,this._$EM(),r}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};x.elementStyles=[],x.shadowRootOptions={mode:"open"},x[G("elementProperties")]=new Map,x[G("finalized")]=new Map,ns?.({ReactiveElement:x}),(ot.reactiveElementVersions??=[]).push("2.1.2");var Lt=globalThis,We=s=>s,nt=Lt.trustedTypes,Be=nt?nt.createPolicy("lit-html",{createHTML:s=>s}):void 0,It="$lit$",E=`lit$${Math.random().toFixed(9).slice(2)}$`,Ut="?"+E,ls=`<${Ut}>`,z=document,Y=()=>z.createComment(""),X=s=>s===null||typeof s!="object"&&typeof s!="function",Ft=Array.isArray,Ye=s=>Ft(s)||typeof s?.[Symbol.iterator]=="function",Pt=`[ 	
\f\r]`,K=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Je=/-->/g,qe=/>/g,N=RegExp(`>|${Pt}(?:([^\\s"'>=/]+)(${Pt}*=${Pt}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Ve=/'/g,Ge=/"/g,Xe=/^(?:script|style|textarea|title)$/i,Dt=s=>(t,...e)=>({_$litType$:s,strings:t,values:e}),l=Dt(1),Zs=Dt(2),ta=Dt(3),T=Symbol.for("lit-noChange"),u=Symbol.for("lit-nothing"),Ke=new WeakMap,R=z.createTreeWalker(z,129);function Qe(s,t){if(!Ft(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return Be!==void 0?Be.createHTML(t):t}var Ze=(s,t)=>{let e=s.length-1,r=[],a,i=t===2?"<svg>":t===3?"<math>":"",o=K;for(let c=0;c<e;c++){let d=s[c],p,f,n=-1,h=0;for(;h<d.length&&(o.lastIndex=h,f=o.exec(d),f!==null);)h=o.lastIndex,o===K?f[1]==="!--"?o=Je:f[1]!==void 0?o=qe:f[2]!==void 0?(Xe.test(f[2])&&(a=RegExp("</"+f[2],"g")),o=N):f[3]!==void 0&&(o=N):o===N?f[0]===">"?(o=a??K,n=-1):f[1]===void 0?n=-2:(n=o.lastIndex-f[2].length,p=f[1],o=f[3]===void 0?N:f[3]==='"'?Ge:Ve):o===Ge||o===Ve?o=N:o===Je||o===qe?o=K:(o=N,a=void 0);let m=o===N&&s[c+1].startsWith("/>")?" ":"";i+=o===K?d+ls:n>=0?(r.push(p),d.slice(0,n)+It+d.slice(n)+E+m):d+E+(n===-2?c:m)}return[Qe(s,i+(s[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),r]},Q=class s{constructor({strings:t,_$litType$:e},r){let a;this.parts=[];let i=0,o=0,c=t.length-1,d=this.parts,[p,f]=Ze(t,e);if(this.el=s.createElement(p,r),R.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(a=R.nextNode())!==null&&d.length<c;){if(a.nodeType===1){if(a.hasAttributes())for(let n of a.getAttributeNames())if(n.endsWith(It)){let h=f[o++],m=a.getAttribute(n).split(E),b=/([.?@])?(.*)/.exec(h);d.push({type:1,index:i,name:b[2],strings:m,ctor:b[1]==="."?dt:b[1]==="?"?ut:b[1]==="@"?ct:P}),a.removeAttribute(n)}else n.startsWith(E)&&(d.push({type:6,index:i}),a.removeAttribute(n));if(Xe.test(a.tagName)){let n=a.textContent.split(E),h=n.length-1;if(h>0){a.textContent=nt?nt.emptyScript:"";for(let m=0;m<h;m++)a.append(n[m],Y()),R.nextNode(),d.push({type:2,index:++i});a.append(n[h],Y())}}}else if(a.nodeType===8)if(a.data===Ut)d.push({type:2,index:i});else{let n=-1;for(;(n=a.data.indexOf(E,n+1))!==-1;)d.push({type:7,index:i}),n+=E.length-1}i++}}static createElement(t,e){let r=z.createElement("template");return r.innerHTML=t,r}};function M(s,t,e=s,r){if(t===T)return t;let a=r!==void 0?e._$Co?.[r]:e._$Cl,i=X(t)?void 0:t._$litDirective$;return a?.constructor!==i&&(a?._$AO?.(!1),i===void 0?a=void 0:(a=new i(s),a._$AT(s,e,r)),r!==void 0?(e._$Co??=[])[r]=a:e._$Cl=a),a!==void 0&&(t=M(s,a._$AS(s,t.values),a,r)),t}var lt=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:r}=this._$AD,a=(t?.creationScope??z).importNode(e,!0);R.currentNode=a;let i=R.nextNode(),o=0,c=0,d=r[0];for(;d!==void 0;){if(o===d.index){let p;d.type===2?p=new U(i,i.nextSibling,this,t):d.type===1?p=new d.ctor(i,d.name,d.strings,this,t):d.type===6&&(p=new ht(i,this,t)),this._$AV.push(p),d=r[++c]}o!==d?.index&&(i=R.nextNode(),o++)}return R.currentNode=z,a}p(t){let e=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(t,r,e),e+=r.strings.length-2):r._$AI(t[e])),e++}},U=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,r,a){this.type=2,this._$AH=u,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=r,this.options=a,this._$Cv=a?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=M(this,t,e),X(t)?t===u||t==null||t===""?(this._$AH!==u&&this._$AR(),this._$AH=u):t!==this._$AH&&t!==T&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):Ye(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==u&&X(this._$AH)?this._$AA.nextSibling.data=t:this.T(z.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:r}=t,a=typeof r=="number"?this._$AC(t):(r.el===void 0&&(r.el=Q.createElement(Qe(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===a)this._$AH.p(e);else{let i=new lt(a,this),o=i.u(this.options);i.p(e),this.T(o),this._$AH=i}}_$AC(t){let e=Ke.get(t.strings);return e===void 0&&Ke.set(t.strings,e=new Q(t)),e}k(t){Ft(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,r,a=0;for(let i of t)a===e.length?e.push(r=new s(this.O(Y()),this.O(Y()),this,this.options)):r=e[a],r._$AI(i),a++;a<e.length&&(this._$AR(r&&r._$AB.nextSibling,a),e.length=a)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let r=We(t).nextSibling;We(t).remove(),t=r}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},P=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,r,a,i){this.type=1,this._$AH=u,this._$AN=void 0,this.element=t,this.name=e,this._$AM=a,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=u}_$AI(t,e=this,r,a){let i=this.strings,o=!1;if(i===void 0)t=M(this,t,e,0),o=!X(t)||t!==this._$AH&&t!==T,o&&(this._$AH=t);else{let c=t,d,p;for(t=i[0],d=0;d<i.length-1;d++)p=M(this,c[r+d],e,d),p===T&&(p=this._$AH[d]),o||=!X(p)||p!==this._$AH[d],p===u?t=u:t!==u&&(t+=(p??"")+i[d+1]),this._$AH[d]=p}o&&!a&&this.j(t)}j(t){t===u?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},dt=class extends P{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===u?void 0:t}},ut=class extends P{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==u)}},ct=class extends P{constructor(t,e,r,a,i){super(t,e,r,a,i),this.type=5}_$AI(t,e=this){if((t=M(this,t,e,0)??u)===T)return;let r=this._$AH,a=t===u&&r!==u||t.capture!==r.capture||t.once!==r.once||t.passive!==r.passive,i=t!==u&&(r===u||a);a&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},ht=class{constructor(t,e,r){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(t){M(this,t)}},tr={M:It,P:E,A:Ut,C:1,L:Ze,R:lt,D:Ye,V:M,I:U,H:P,N:ut,U:ct,B:dt,F:ht},ds=Lt.litHtmlPolyfillSupport;ds?.(Q,U),(Lt.litHtmlVersions??=[]).push("3.3.3");var er=(s,t,e)=>{let r=e?.renderBefore??t,a=r._$litPart$;if(a===void 0){let i=e?.renderBefore??null;r._$litPart$=a=new U(t.insertBefore(Y(),i),i,void 0,e??{})}return a._$AI(s),a};var jt=globalThis,g=class extends x{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=er(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return T}};g._$litElement$=!0,g.finalized=!0,jt.litElementHydrateSupport?.({LitElement:g});var us=jt.litElementPolyfillSupport;us?.({LitElement:g});(jt.litElementVersions??=[]).push("4.2.2");function cs(s,t,e){let r=s.elementFromPoint?.(t,e)??null;for(;r?.shadowRoot?.elementFromPoint;){let a=r.shadowRoot.elementFromPoint(t,e);if(!a||a===r)break;r=a}return r}function hs(s){for(let t=s;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var rr=(s,t,e)=>hs(cs(s,t,e));function sr({root:s=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:r=()=>{}}={}){let a=null,i=()=>{let{handle:n,pointerId:h}=a;n.removeEventListener("pointermove",o),n.removeEventListener("pointerup",c),n.removeEventListener("pointercancel",d),n.removeEventListener("lostpointercapture",d),s.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(h)}catch{}a=null};function o(n){if(!(!a||n.pointerId!==a.pointerId)){if(!a.moving){if(Math.hypot(n.clientX-a.x,n.clientY-a.y)<8)return;a.moving=!0,t(a.room)}n.preventDefault(),e(rr(s,n.clientX,n.clientY))}}function c(n){if(!a||n.pointerId!==a.pointerId)return;let{room:h,moving:m}=a;if(i(),!m)return;let b=_=>{_.stopPropagation(),_.preventDefault()};s.addEventListener("click",b,!0),setTimeout(()=>s.removeEventListener("click",b,!0),0),r(h,rr(s,n.clientX,n.clientY))}function d(n){if(!a||n&&n.pointerId!==void 0&&n.pointerId!==a.pointerId)return;let{room:h,moving:m}=a;i(),m&&r(h,null)}function p(n){n.key==="Escape"&&d()}function f(n){if(a||n.isPrimary===!1||n.button>0)return;let h=n.composedPath().find(m=>m.dataset?.dragRoom);if(h){a={handle:h,room:h.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{h.setPointerCapture?.(n.pointerId)}catch{}h.addEventListener("pointermove",o),h.addEventListener("pointerup",c),h.addEventListener("pointercancel",d),h.addEventListener("lostpointercapture",d),s.addEventListener("keydown",p,!0)}}return{begin:f,cancel:()=>d(),active:()=>!!a?.moving}}var pt=[],ir=s=>String(s).split("/").filter(Boolean);function w(s){let{id:t,path:e,title:r,render:a}=s??{};if(typeof t!="string"||!t||t==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof a!="function")throw new Error(`the screen '${t}' has a title and a render`);let i=ir(e);if(i.length===0)throw new Error(`the screen '${t}' has a path`);let o=c=>c.map(d=>d.startsWith(":")?":":d).join("/");for(let c of pt){if(c.id===t)throw new Error(`the screen '${t}' is registered twice`);if(o(c.segments)===o(i))throw new Error(`the screens '${c.id}' and '${t}' have the same path`)}pt.push({id:t,segments:i,title:r,render:a})}function Ht(s){return pt.find(t=>t.id===s)??null}var ft="#/",mt=Object.freeze({screen:"home",params:Object.freeze({}),address:ft});function S(s,t={}){let e=Ht(s);if(!e)throw new Error(`there is no screen '${s}'`);return`#/${e.segments.map(a=>{if(!a.startsWith(":"))return a;let i=t[a.slice(1)];if(typeof i!="string"||!i)throw new Error(`the screen '${s}' needs '${a.slice(1)}'`);return encodeURIComponent(i)}).join("/")}`}function ar(s){let t;try{t=ir(String(s??"").replace(/^#/,"")).map(e=>decodeURIComponent(e))}catch{return mt}for(let e of pt){if(e.segments.length!==t.length)continue;let r={};if(e.segments.every((i,o)=>i.startsWith(":")?(r[i.slice(1)]=t[o],!0):i===t[o]))return{screen:e.id,params:r,address:S(e.id,r)}}return mt}function or(s=globalThis){let t=new Set,e=()=>ar(s.location?.hash??""),r=()=>{let a=e();for(let i of[...t])i(a)};return{route:e,open(a){let i=ar(a);i.address!==e().address&&(s.history.pushState({chorus:!0},"",i.address),r())},back(){if(e().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",ft),r()}},watch(a){let i=o=>a(o);return t.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),t.add(i),i(e()),()=>{t.delete(i),t.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var ps=(s,t)=>Nt("../",s,t),k=s=>typeof s=="string"&&s?s:null,ms=["playing","paused","buffering"];function nr(s,t=ps){let e=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let a of e){if(!a||typeof a!="object"||typeof a.id!="string"||!a.id)continue;let i=a.now_playing&&typeof a.now_playing=="object"?a.now_playing:null,o=i?k(i.art_url):null;r.set(a.id,{source:k(a.source),nowPlaying:i&&{title:k(i.title),artist:k(i.artist),album:k(i.album),state:ms.includes(i.state)?i.state:null,via:k(i.via),artwork:o?t(a.id,o):null}})}return r}var Bt={source:null,nowPlaying:null};function fs(s){let t=s&&Array.isArray(s.inputs)?s.inputs:[],e=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return t.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:e.get(r)??r}))}function gs(s){let t=s&&s.sound&&typeof s.sound=="object"?s.sound:{},e=a=>Number.isInteger(a)?a:null,r=a=>typeof a=="boolean"?a:null;return{bass:e(t.bass),treble:e(t.treble),loudness:r(t.loudness),night:r(t.night),speech:r(t.speech)}}function vs(s){let t=s&&typeof s=="object"?s:{},e=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:L(t.limit),effectiveLimit:L(t.effective_limit),quietEnabled:typeof t.quiet_enabled=="boolean"?t.quiet_enabled:null,windows:(Array.isArray(t.quiet)?t.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),start:e(r.start),end:e(r.end),limit:L(r.limit),active:r.active===!0}))}}function lr(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(e=>e&&typeof e.input=="string"&&e.input&&typeof e.target=="string").map(e=>({input:e.input,target:e.target,enabled:e.enabled===!0,stopOnStandby:e.stop_on_standby!==!1,lowLatency:e.low_latency!==!1}))}function dr(s){let t=s&&Array.isArray(s.alarms)?s.alarms:[],e=r=>Number.isInteger(r)&&r>=0?r:0;return t.filter(r=>r&&typeof r.alarm=="string"&&r.alarm&&typeof r.target=="string").map(r=>({id:r.alarm,target:r.target,time:typeof r.time=="string"?r.time:"",days:(Array.isArray(r.days)?r.days:[]).filter(a=>typeof a=="string"),source:typeof r.source=="string"?r.source:"",volume:L(r.volume)??0,rampS:e(r.ramp_s),durationMin:e(r.duration_min),enabled:r.enabled===!0,ringing:r.ringing===!0}))}function ur(s){return(s&&Array.isArray(s.sleep)?s.sleep:[]).filter(e=>e&&typeof e.target=="string"&&e.target).map(e=>({target:e.target,minutes:Number.isInteger(e.minutes)?e.minutes:null,remainingS:Number.isInteger(e.remaining_s)&&e.remaining_s>=0?e.remaining_s:null}))}function cr(s){return(s&&Array.isArray(s.stored_sources)?s.stored_sources:[]).filter(e=>e&&typeof e.id=="string"&&e.id&&typeof e.kind=="string").map(e=>({id:e.id,kind:e.kind,value:typeof e.value=="string"?e.value:"",name:typeof e.name=="string"&&e.name?e.name:e.id}))}function hr(s){return!s||!Array.isArray(s.chimes)?null:s.chimes.filter(t=>typeof t=="string"&&t)}function pr(s){let t=s&&s.soloist&&typeof s.soloist=="object"?s.soloist:null;return t?(Array.isArray(t.receivers)?t.receivers:[]).filter(e=>e&&e.state==="running"&&typeof e.target=="string"&&e.target).map(e=>e.target):null}function Jt(s){return(s&&Array.isArray(s.speakers)?s.speakers:[]).filter(e=>e&&typeof e=="object"&&typeof e.id=="string"&&e.id).map(e=>{let r=e.named===!0,a=k(e.room);return{id:e.id,name:k(e.name)??e.id,named:r,room:a,isNew:!r&&a===null,present:e.present===!0,software:k(e.software),link:k(e.link)??"unknown",key:k(e.key),roles:(Array.isArray(e.roles)?e.roles:[]).filter(i=>typeof i=="string"&&i)}})}function mr(s){return(s&&Array.isArray(s.key_changes)?s.key_changes:[]).filter(e=>e&&typeof e=="object"&&typeof e.id=="string"&&e.id).map(e=>({id:e.id,pinned:k(e.pinned),offered:k(e.offered)}))}function F(s,t){return(Array.isArray(s)?s:[]).find(e=>e.id===t)??null}function bs(s,t,e){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let r=Array.isArray(s.bond)?s.bond:[],a=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:L(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:gs(s),limits:vs(s),group:a,...a===s.id&&e.get(a)||Bt,bond:r.filter(i=>i&&typeof i.endpoint=="string"&&typeof i.role=="string").map(i=>({endpoint:i.endpoint,name:t.get(i.endpoint)??i.endpoint,role:i.role}))}}function fr(s,t){let e=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],a=new Map(r.filter(o=>o&&typeof o.id=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.id,o.name])),i=nr(s,t);return e.map(o=>bs(o,a,i)).filter(Boolean)}function L(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function $s(s,t){let e=new Map(fr(s,t).map(n=>[n.id,n.name])),r=nr(s,t),a=n=>({id:n,name:e.get(n)??n}),i=n=>Array.isArray(n)?n:[],o=n=>i(n).filter(h=>typeof h=="string"&&h).map(a),c=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,d=i(s?.groups).filter(c),p=i(s?.saved_groups).filter(c),f=new Set(p.map(n=>n.id));return[...p.map(n=>{let h=d.find(m=>m.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:o(n.zones),rooms:h?o(h.zones):[],volume:h?L(h.volume):null,...h&&r.get(n.id)||Bt}}),...d.filter(n=>n.kind==="live"&&!f.has(n.id)).map(n=>{let h=o(n.zones);return{id:n.id,name:h.map(m=>m.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:h,volume:L(n.volume),...r.get(n.id)??Bt}})]}var Wt=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function gr(s){let t=new Set,e=null,r=[],a=[],i=[],o="connecting",c=!1,d=null,p=()=>({state:e,rooms:r,groups:a,inputs:i,status:o}),f=()=>{let v=p();for(let y of[...t])y(v)},n=v=>{e=v,r=fr(v,s.artwork),a=$s(v,s.artwork),i=fs(v)};function h(){d||(d=s.events({onState(v){Wt(v)&&(c=!0,n(v),f())},onStatus(v){o!==v&&(o=v,f())}}),s.state().then(v=>{c||!Wt(v)||(n(v),f())},()=>{}))}function m(){d?.(),d=null}async function b(v){let y=await s.command(v);return y.signedOut&&o!=="signed-out"&&(o="signed-out",f()),y.ok&&Wt(y.state)&&(!e||y.state.serial>e.serial)&&(n(y.state),f()),y}function _(v){return t.add(v),v(p()),()=>t.delete(v)}return{start:h,stop:m,command:b,subscribe:_,view:p}}var gt=s=>`alarm:${s}`,vr="alarms:draft",br=s=>`stored:${s}`,$r="stored:draft:",_r=s=>`sleep:${s}`,yr="sleep:draft:",qt={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},vt={url:"Stream URL",spotify:"Spotify URI"},_s=Object.freeze({alarm:"",target:"",time:"07:00",days:Object.freeze(["mon","tue","wed","thu","fri"]),source:"",volume:300,rampS:30,durationMin:60,enabled:!0}),ys=Object.freeze({id:"",name:"",kind:"url",value:""}),ks=Object.freeze({target:"",minutes:30}),Vt=s=>`${Math.round(s/10)}%`,ws=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),kr=(s,t)=>Math.min(t,Math.max(0,Math.round(Number(s)||0)));function Ss(s){let t=Math.max(0,Math.floor(s)),e=Math.floor(t/3600),r=Math.floor(t%3600/60);return e>0?`${e} h ${r} min left`:r>0?`${r} min ${t%60} s left`:`${t} s left`}var Kt=class extends g{static properties={known:{type:Boolean},heard:{attribute:!1},alarms:{attribute:!1},stored:{attribute:!1},sleep:{attribute:!1},chimes:{attribute:!1},receivers:{attribute:!1},inputs:{attribute:!1},rooms:{attribute:!1},savedGroups:{attribute:!1},formedGroups:{attribute:!1},refusals:{attribute:!1},refusalFields:{attribute:!1},_alarm:{state:!0},_source:{state:!0},_timer:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.known=!1,this.heard=null,this.alarms=[],this.stored=[],this.sleep=[],this.chimes=null,this.receivers=null,this.inputs=[],this.rooms=[],this.savedGroups=[],this.formedGroups=[],this.refusals={},this.refusalFields={},this._alarm={..._s},this._source={...ys},this._timer={...ks},this.clock=()=>globalThis.performance.now(),this._heardAt=0,this._ticker=null}disconnectedCallback(){super.disconnectedCallback(),this._tickEvery(!1)}willUpdate(t){t.has("heard")&&(this._heardAt=this.clock())}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-holds]")){let e=t.dataset.holds;t.value!==e&&(t.value=e)}this._tickEvery(this.isConnected&&(this.sleep??[]).some(t=>t.remainingS!==null))}_tickEvery(t){t!==(this._ticker!==null)&&(t?this._ticker=globalThis.setInterval(()=>this.tick(),1e3):(globalThis.clearInterval(this._ticker),this._ticker=null))}tick(){this.requestUpdate()}_left(t){let e=Math.floor(Math.max(0,this.clock()-this._heardAt)/1e3);return Math.max(0,t.remainingS-e)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:t,body:e},bubbles:!0,composed:!0}))}_refusal(t){let e=this.refusals?.[t]??"";if(!e)return l`<p role="alert"></p>`;let r=this.refusalFields?.[t]??"";return l`<p role="alert" data-refusal-field=${r||u}>Refused${r?` (${r})`:""}: ${e}</p>`}_place(t){return[...this.rooms??[],...this.savedGroups??[],...this.formedGroups??[]].find(r=>r.id===t)?.name??t}_storedOf(t){return t.startsWith("stored:")?(this.stored??[]).find(e=>e.id===t.slice(7))??null:null}_sourceName(t){if(t.startsWith("chime:"))return`Chime: ${t.slice(6)}`;if(t.startsWith("line-in:"))return`Input: ${(this.inputs??[]).find(a=>a.source===t)?.label??t.slice(8)}`;let e=this._storedOf(t);return e?`${vt[e.kind]??e.kind}: ${e.name}`:t}_unplayable(t,e){if(t.startsWith("chime:"))return this.chimes!==null&&!this.chimes.includes(t.slice(6))?`This server has no chime "${t.slice(6)}".`:"";if(t.startsWith("line-in:"))return(this.inputs??[]).some(r=>r.source===t)?"":`The input ${t.slice(8)} is not offered now: its speaker is not connected.`;if(t.startsWith("stored:")){let r=this._storedOf(t);if(!r)return`This server has no stored source "${t.slice(7)}".`;if(r.kind!=="spotify")return"";if(this.receivers===null)return"This server runs no Spotify receiver.";let a=(this.savedGroups??[]).some(i=>i.id===e);return this.receivers.includes(`${a?"group":"room"}:${e}`)?"":`No Spotify receiver is running for ${this._place(e)}.`}return"This is not a source an alarm plays."}_alarmOf(t){return(this.alarms??[]).find(e=>e.id===t)??null}_sendable(t,e={}){return Ot({...t,alarm:t.id,...e})}_onSwitch(t){let e=this._alarmOf(t.currentTarget.dataset.alarm);e&&this._ask(gt(e.id),this._sendable(e,{enabled:!e.enabled}))}_onStop(t){let e=t.currentTarget.dataset.alarm;this._ask(gt(e),Ce(e))}_onDelete(t){let e=t.currentTarget.dataset.alarm;this._ask(gt(e),Oe(e))}_onEdit(t){let e=this._alarmOf(t.currentTarget.dataset.alarm);if(!e)return;let{id:r,ringing:a,...i}=e;this._alarm={alarm:r,...i}}_alarmRow(t){let e=t.days.length===0?"once":A.filter(i=>t.days.includes(i)).map(i=>qt[i][0]).join(" "),r=t.durationMin===0?"until stopped":`for ${t.durationMin} min`,a=this._unplayable(t.source,t.target);return l`
      <li data-alarm=${t.id} ?data-ringing=${t.ringing}>
        <h4>${t.id}</h4>
        <p data-value="when">${t.time}, ${e}</p>
        <p data-value="what">
          ${this._sourceName(t.source)} in ${this._place(t.target)}, to ${Vt(t.volume)} over ${t.rampS} s,
          ${r}
        </p>
        ${a?l`<p data-fallback>${a} The alarm rings the bell chime instead.</p>`:u}
        <div class="row">
          <button
            type="button"
            data-alarm=${t.id}
            aria-label="Alarm ${t.id}"
            aria-pressed=${t.enabled?"true":"false"}
            @click=${this._onSwitch}
          >
            Alarm
          </button>
          <span data-value="enabled">${t.enabled?"On":"Off"}</span>
          ${t.ringing?l`<span data-value="ringing">Ringing now</span>
                <button type="button" data-alarm=${t.id} aria-label="Stop alarm ${t.id}" @click=${this._onStop}>
                  Stop
                </button>`:u}
          <button type="button" data-alarm=${t.id} aria-label="Edit alarm ${t.id}" @click=${this._onEdit}>Edit</button>
          <button type="button" data-alarm=${t.id} aria-label="Delete alarm ${t.id}" @click=${this._onDelete}>
            Delete
          </button>
        </div>
        ${this._refusal(gt(t.id))}
      </li>
    `}_offeredSources(){let t=e=>(this.stored??[]).filter(r=>r.kind===e).map(r=>({value:`stored:${r.id}`,name:r.name}));return[{kind:"chime",label:"Chimes",options:(this.chimes??[]).map(e=>({value:`chime:${e}`,name:e}))},{kind:"line-in",label:"Inputs",options:(this.inputs??[]).map(e=>({value:e.source,name:e.label}))},{kind:"url",label:"Stored stream URLs",options:t("url")},{kind:"spotify",label:"Stored Spotify URIs",options:t("spotify")}]}_alarmDraft(){let t=this._alarm,e=[...this.rooms??[],...this.savedGroups??[]],r=this._offeredSources().flatMap(a=>a.options)[0];return{...t,target:t.target||(e[0]?.id??""),source:t.source||(r?.value??"")}}_setAlarm(t){this._alarm={...this._alarm,...t}}_onAlarmText(t){this._setAlarm({alarm:t.target.value.trim()})}_onAlarmChoice(t){this._setAlarm({[t.target.dataset.field]:t.target.value})}_onAlarmTime(t){if(!ws(t.target.value)){t.target.value=this._alarm.time;return}this._setAlarm({time:t.target.value})}_onAlarmDay(t){let e=t.currentTarget.dataset.day,r=this._alarm.days.includes(e)?this._alarm.days.filter(a=>a!==e):A.filter(a=>a===e||this._alarm.days.includes(a));this._setAlarm({days:r})}_onAlarmVolume(t){this._setAlarm({volume:Number(t.target.value)})}_onAlarmCount(t){let{field:e,max:r}=t.target.dataset,a=kr(t.target.value,Number(r));t.target.value=String(a),this._setAlarm({[e]:a})}_onAlarmEnabled(){this._setAlarm({enabled:!this._alarm.enabled})}_onSave(){this._ask(vr,Ot(this._alarmDraft()))}_kindNotes(t){let e=[];this.chimes===null&&e.push(["chime","This server does not say which chimes it has, so none is offered here."]),(this.inputs??[]).length===0&&e.push(["line-in","No input is offered now: no speaker with a line-in is connected."]);let r=new Set((this.stored??[]).map(o=>o.kind));r.has("url")||e.push(["url","No stream URL is stored: add one under Stored sources."]),r.has("spotify")?this.receivers===null&&e.push(["spotify","This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]):e.push(["spotify","No Spotify URI is stored: add one under Stored sources."]);let a=t.source?this._unplayable(t.source,t.target):"",i=this._storedOf(t.source)?.kind==="spotify"?"spotify":"chosen";return a&&!(i==="spotify"&&this.receivers===null)&&e.push([i,`${a} The alarm would ring the bell chime instead.`]),e.map(([o,c])=>l`<p data-unavailable=${o}>${c}</p>`)}_alarmForm(){let t=this._alarmDraft(),e=this.rooms??[],r=this.savedGroups??[],a=[...e,...r],i=this._offeredSources(),o=n=>l`<option value=${n.value}>${n.name}</option>`,c=n=>l`<option value=${n.id}>${n.name}</option>`,d=i.some(n=>n.options.some(h=>h.value===t.source)),p=this._alarmOf(t.alarm)!==null,f=t.alarm!==""&&t.target!==""&&t.source!=="";return l`
      <div class="draft" data-draft="alarm">
        <div class="row">
          <label for="alarm-name">Name</label>
          <input
            id="alarm-name"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            .value=${t.alarm}
            aria-label="Alarm name"
            @input=${this._onAlarmText}
          />
          <p>Lower-case letters, digits and "-". An alarm with this name is replaced.</p>
        </div>
        <div class="row">
          <label for="alarm-target">Rings in</label>
          <select id="alarm-target" data-field="target" data-holds=${t.target} aria-label="Alarm target" @change=${this._onAlarmChoice}>
            ${a.some(n=>n.id===t.target)||!t.target?u:l`<option value=${t.target}>${t.target} (not on this server now)</option>`}
            ${e.length===0?u:l`<optgroup label="Rooms">${e.map(c)}</optgroup>`}
            ${r.length===0?u:l`<optgroup label="Saved groups">${r.map(c)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${t.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${A.map(n=>l`<button
                type="button"
                data-day=${n}
                aria-label="${qt[n][1]}, the alarm"
                aria-pressed=${t.days.includes(n)?"true":"false"}
                @click=${this._onAlarmDay}
              >
                ${qt[n][0]}
              </button>`)}
          <p data-value="days">${t.days.length===0?"No day: it rings once, at the next such time.":"It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${t.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${d||!t.source?u:l`<option value=${t.source}>${t.source} (not on this server now)</option>`}
            ${i.map(n=>n.options.length===0?u:l`<optgroup label=${n.label} data-kind=${n.kind}>${n.options.map(o)}</optgroup>`)}
          </select>
        </div>
        ${this._kindNotes(t)}
        <div class="row">
          <label for="alarm-volume">Volume</label>
          <input
            id="alarm-volume"
            type="range"
            min="0"
            max="1000"
            step="1"
            .value=${String(t.volume)}
            aria-label="Alarm volume"
            aria-valuetext=${Vt(t.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${Vt(t.volume)}</span>
        </div>
        <div class="row">
          <label for="alarm-ramp">Rises over, seconds</label>
          <input
            id="alarm-ramp"
            type="number"
            min="0"
            max=${et}
            step="1"
            data-field="rampS"
            data-max=${et}
            .value=${String(t.rampS)}
            aria-label="Alarm ramp, seconds"
            @change=${this._onAlarmCount}
          />
          <label for="alarm-duration">Plays for, minutes</label>
          <input
            id="alarm-duration"
            type="number"
            min="0"
            max=${rt}
            step="1"
            data-field="durationMin"
            data-max=${rt}
            .value=${String(t.durationMin)}
            aria-label="Alarm duration, minutes"
            @change=${this._onAlarmCount}
          />
          <p>0 minutes plays until it is stopped.</p>
        </div>
        <div class="row">
          <button type="button" aria-label="Alarm switched on" aria-pressed=${t.enabled?"true":"false"} @click=${this._onAlarmEnabled}>
            Switched on
          </button>
          <button type="button" aria-label="Save alarm" ?disabled=${!f} @click=${this._onSave}>Save alarm</button>
          <p>${p?`Saving replaces the alarm "${t.alarm}".`:"Nothing is sent until it is saved."}</p>
        </div>
        ${this._refusal(vr)}
      </div>
    `}_onForget(t){let e=t.currentTarget.dataset.stored;this._ask(br(e),Re(e))}_onSourceField(t){this._source={...this._source,[t.target.dataset.field]:t.target.value.trim()}}_onStore(){let{id:t,kind:e,value:r,name:a}=this._source;this._ask($r,Ne(t,e,r,a||t))}_storedRow(t){return l`
      <li data-stored=${t.id}>
        <h4>${t.name}</h4>
        <p><span data-value="kind">${vt[t.kind]??t.kind}</span>, <span data-id>${t.id}</span></p>
        <p data-value="value">${t.value}</p>
        <div class="row">
          <button type="button" data-stored=${t.id} aria-label="Forget stored source ${t.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(br(t.id))}
      </li>
    `}_storedForm(){let t=this._source,e=t.kind==="spotify";return l`
      <div class="draft" data-draft="stored">
        <div class="row">
          <label for="stored-kind">Kind</label>
          <select id="stored-kind" data-field="kind" data-holds=${t.kind} aria-label="Stored source kind" @change=${this._onSourceField}>
            <option value="url">${vt.url}</option>
            <option value="spotify">${vt.spotify}</option>
          </select>
          <label for="stored-id">Id</label>
          <input
            id="stored-id"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="id"
            .value=${t.id}
            aria-label="Stored source id"
            @input=${this._onSourceField}
          />
        </div>
        <div class="row">
          <label for="stored-name">Name</label>
          <input id="stored-name" type="text" data-field="name" .value=${t.name} aria-label="Stored source name" @input=${this._onSourceField} />
        </div>
        <div class="row">
          <label for="stored-value">${e?"Spotify URI":"Address"}</label>
          <input
            id="stored-value"
            type="text"
            autocomplete="off"
            autocapitalize="none"
            spellcheck="false"
            data-field="value"
            .value=${t.value}
            placeholder=${e?"spotify:playlist:...":"https://..."}
            aria-label="Stored source address"
            @input=${this._onSourceField}
          />
        </div>
        <p>
          ${e?"A playlist, an album, a track or an episode, as the Spotify app shares it: spotify:playlist:<id>.":"An http:// or https:// address of a stream. Everyone who can open this app can read it: do not store one with a password in it."}
        </p>
        <div class="row">
          <button type="button" aria-label="Store source" ?disabled=${!t.id||!t.value} @click=${this._onStore}>
            Store source
          </button>
          <p>A source stored under an id already taken replaces it.</p>
        </div>
        ${this._refusal($r)}
      </div>
    `}_sleepTargets(){return[...this.rooms??[],...this.formedGroups??[]]}_onCancel(t){let e=t.currentTarget.dataset.target;this._ask(_r(e),Ct(e,0))}_onSleepTarget(t){this._timer={...this._timer,target:t.target.value}}_onSleepMinutes(t){let e=kr(t.target.value,st);t.target.value=String(e),this._timer={...this._timer,minutes:e}}_onSleep(){let t=this._timer.target||(this._sleepTargets()[0]?.id??"");t&&this._ask(yr,Ct(t,this._timer.minutes))}_sleepRow(t){let e=this._place(t.target),r=t.remainingS===null?`${t.minutes??"?"} min asked for`:Ss(this._left(t));return l`
      <li data-sleep=${t.target}>
        <h4>${e}</h4>
        <div class="row">
          <span class="figure" data-value="left">${r}</span>
          <button type="button" data-target=${t.target} aria-label="Cancel sleep timer for ${e}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(_r(t.target))}
      </li>
    `}_sleepForm(){let t=this.rooms??[],e=this.formedGroups??[],r=this._timer.target||(this._sleepTargets()[0]?.id??""),a=i=>l`<option value=${i.id}>${i.name}</option>`;return l`
      <div class="draft" data-draft="sleep">
        <div class="row">
          <label for="sleep-target">For</label>
          <select id="sleep-target" data-holds=${r} aria-label="Sleep timer target" @change=${this._onSleepTarget}>
            ${t.length===0?u:l`<optgroup label="Rooms">${t.map(a)}</optgroup>`}
            ${e.length===0?u:l`<optgroup label="Groups playing now">${e.map(a)}</optgroup>`}
          </select>
          <label for="sleep-minutes">Minutes</label>
          <input
            id="sleep-minutes"
            type="number"
            min="0"
            max=${st}
            step="1"
            .value=${String(this._timer.minutes)}
            aria-label="Sleep timer minutes"
            @change=${this._onSleepMinutes}
          />
          <button type="button" aria-label="Start sleep timer" ?disabled=${!r} @click=${this._onSleep}>Start</button>
        </div>
        <p>It fades the room out and stops it when the time is up. 0 minutes cancels the timer it has.</p>
        ${this._refusal(yr)}
      </div>
    `}render(){if(!this.known)return l`<p role="status" data-missing>Reading this server's alarms.</p>`;let t=this.alarms??[],e=this.stored??[],r=this.sleep??[];return l`
      <h2>Alarms</h2>
      <p>An alarm rings in its room or its saved group on the server's own clock, and rises from silence to its volume.</p>
      ${t.length===0?l`<p role="status" data-none="alarms">This server has no alarm.</p>`:l`<ul aria-label="Alarms">
            ${t.map(a=>this._alarmRow(a))}
          </ul>`}
      <h3>Set an alarm</h3>
      ${this._alarmForm()}

      <h2>Stored sources</h2>
      <p>A stream URL or a Spotify URI the server keeps for an alarm to play.</p>
      ${e.length===0?l`<p role="status" data-none="stored">This server has no stored source.</p>`:l`<ul aria-label="Stored sources">
            ${e.map(a=>this._storedRow(a))}
          </ul>`}
      <h3>Store a source</h3>
      ${this._storedForm()}

      <h2>Sleep timers</h2>
      ${r.length===0?l`<p role="status" data-none="sleep">No sleep timer is running.</p>`:l`<ul aria-label="Sleep timers">
            ${r.map(a=>this._sleepRow(a))}
          </ul>`}
      <h3>Start a sleep timer</h3>
      ${this._sleepForm()}
    `}};customElements.define("chorus-alarms",Kt);var Yt="alarms",Gt=s=>s.map(({id:t,name:e})=>({id:t,name:e}));w({id:Yt,path:"alarms",title:()=>"Alarms and sleep timers",render:(s,{view:t,refusals:e,refusalFields:r})=>{let a=t.groups??[];return l`
      <chorus-alarms
        .known=${t.state!==null}
        .heard=${t.state}
        .alarms=${dr(t.state)}
        .stored=${cr(t.state)}
        .sleep=${ur(t.state)}
        .chimes=${hr(t.state)}
        .receivers=${pr(t.state)}
        .inputs=${t.inputs??[]}
        .rooms=${Gt(t.rooms)}
        .savedGroups=${Gt(a.filter(i=>i.kind==="saved"))}
        .formedGroups=${Gt(a.filter(i=>i.rooms.length>0))}
        .refusals=${e}
        .refusalFields=${r}
      ></chorus-alarms>
    `}});function Z(s,t){return t.find(e=>e.id===s.group&&e.rooms.some(r=>r.id===s.id))??null}function wr(s,t,e){if(!s||!t)return null;let r=Z(s,e);return t.kind==="alone"?r?tt(s.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?r&&r.id===t.id?null:Et(s.id,t.id):t.kind==="room"?t.id===s.id||r&&r.rooms.some(a=>a.id===t.id)?null:Et(s.id,t.id):null}var Xt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function Sr(s){if(s==="alone")return{kind:"alone"};let t=String(s).indexOf(":");if(t<1)return null;let e=s.slice(0,t),r=s.slice(t+1);return(e==="room"||e==="group")&&r?{kind:e,id:r}:null}function Ar(s,t){let e=Z(s,t);return e?Xt({kind:"group",id:e.id}):"alone"}function xr(s,t,e){return[{value:"alone",label:"Alone"},...e.map(r=>({value:Xt({kind:"group",id:r.id}),label:r.name})),...t.filter(r=>r.id!==s.id&&!Z(r,e)).map(r=>({value:Xt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var Er=s=>`autoplay:${s}`;function As(s){let t=lr(s.state),e=o=>t.find(c=>c.input===o)??null,r=(s.inputs??[]).map(o=>({input:o.id,label:o.label,offered:!0,rule:e(o.id)})),a=new Set(r.map(o=>o.input)),i=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(o=>o&&typeof o.input=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.input,o.name]));return[...r,...t.filter(o=>!a.has(o.input)).map(o=>({input:o.input,label:i.get(o.input)??o.input,offered:!1,rule:o}))]}var Qt=class extends g{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={}}_row(t){return(this.rows??[]).find(e=>e.input===t)??null}_ask(t,e,r){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Er(t.input),body:Te(t.input,e,r,t.rule??{})},bubbles:!0,composed:!0}))}_onSwitch(t){let e=this._row(t.currentTarget.dataset.input);e?.rule&&this._ask(e,e.rule.target,!e.rule.enabled)}_onTarget(t){let e=this._row(t.target.dataset.input),r=t.target.value,a=e?.rule?.target??"";t.target.value=a,!(!e||!r||r===a)&&this._ask(e,r,e.rule?.enabled??!1)}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-input]")){let e=this._row(t.dataset.input)?.rule?.target??"";t.value!==e&&(t.value=e)}}_targets(t){let e=this.rooms??[],r=this.groups??[],a=!t||[...e,...r].some(o=>o.id===t.target),i=o=>l`<option value=${o.id} ?selected=${t?.target===o.id}>${o.name}</option>`;return l`
      ${t?u:l`<option value="" selected>Nowhere yet</option>`}
      ${a?u:l`<option value=${t.target} selected>${t.target} (not on this server now)</option>`}
      ${e.length===0?u:l`<optgroup label="Rooms">${e.map(i)}</optgroup>`}
      ${r.length===0?u:l`<optgroup label="Saved groups">${r.map(i)}</optgroup>`}
    `}_input(t){let{input:e,label:r,offered:a,rule:i}=t,o=this.refusals?.[Er(e)]??"",c=i?i.enabled?"On":"Off":"Choose where it plays, then switch it on";return l`
      <li data-input=${e}>
        <h3>${r}</h3>
        ${r===e?u:l`<p data-id>${e}</p>`}
        ${a?u:l`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${e}
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
          <label for="target-${e}">Plays in</label>
          <select id="target-${e}" data-input=${e} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(i)}
          </select>
        </div>
        <p role="alert">${o?`Refused: ${o}`:u}</p>
      </li>
    `}render(){return this.rows===null?l`<p role="status" data-missing>Reading this server's inputs.</p>`:l`
      <h2>Autoplay</h2>
      <p>An input with a rule that is on plays in its room or its group when its signal arrives.</p>
      ${this.rows.length===0?l`<p role="status" data-none>This server offers no input now, and has no autoplay rule.</p>`:l`<ul aria-label="Inputs">
            ${this.rows.map(t=>this._input(t))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",Qt);var Zt="autoplay",Tr=s=>s.map(({id:t,name:e})=>({id:t,name:e}));w({id:Zt,path:"autoplay",title:()=>"Autoplay",render:(s,{view:t,refusals:e})=>l`
    <chorus-autoplay
      .rows=${t.state===null?null:As(t)}
      .rooms=${Tr(t.rooms)}
      .groups=${Tr((t.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${e}
    ></chorus-autoplay>
  `});var Or={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},bt=s=>(...t)=>({_$litDirective$:s,values:t}),D=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,r){this._$Ct=t,this._$AM=e,this._$Ci=r}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:xs}=tr,Cr=s=>s;var Nr=()=>document.createComment(""),j=(s,t,e)=>{let r=s._$AA.parentNode,a=t===void 0?s._$AB:t._$AA;if(e===void 0){let i=r.insertBefore(Nr(),a),o=r.insertBefore(Nr(),a);e=new xs(i,o,s,s.options)}else{let i=e._$AB.nextSibling,o=e._$AM,c=o!==s;if(c){let d;e._$AQ?.(s),e._$AM=s,e._$AP!==void 0&&(d=s._$AU)!==o._$AU&&e._$AP(d)}if(i!==a||c){let d=e._$AA;for(;d!==i;){let p=Cr(d).nextSibling;Cr(r).insertBefore(d,a),d=p}}}return e},C=(s,t,e=s)=>(s._$AI(t,e),s),Es={},$t=(s,t=Es)=>s._$AH=t,Rr=s=>s._$AH,_t=s=>{s._$AR(),s._$AA.remove()};var zr=(s,t,e)=>{let r=new Map;for(let a=t;a<=e;a++)r.set(s[a],a);return r},yt=bt(class extends D{constructor(s){if(super(s),s.type!==Or.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,t,e){let r;e===void 0?e=t:t!==void 0&&(r=t);let a=[],i=[],o=0;for(let c of s)a[o]=r?r(c,o):o,i[o]=e(c,o),o++;return{values:i,keys:a}}render(s,t,e){return this.dt(s,t,e).values}update(s,[t,e,r]){let a=Rr(s),{values:i,keys:o}=this.dt(t,e,r);if(!Array.isArray(a))return this.ut=o,i;let c=this.ut??=[],d=[],p,f,n=0,h=a.length-1,m=0,b=i.length-1;for(;n<=h&&m<=b;)if(a[n]===null)n++;else if(a[h]===null)h--;else if(c[n]===o[m])d[m]=C(a[n],i[m]),n++,m++;else if(c[h]===o[b])d[b]=C(a[h],i[b]),h--,b--;else if(c[n]===o[b])d[b]=C(a[n],i[b]),j(s,d[b+1],a[n]),n++,b--;else if(c[h]===o[m])d[m]=C(a[h],i[m]),j(s,a[n],a[h]),h--,m++;else if(p===void 0&&(p=zr(o,m,b),f=zr(c,n,h)),p.has(c[n]))if(p.has(c[h])){let _=f.get(o[m]),v=_!==void 0?a[_]:null;if(v===null){let y=j(s,a[n]);C(y,i[m]),d[m]=y}else d[m]=C(v,i[m]),j(s,a[n],v),a[_]=null;m++}else _t(a[h]),h--;else _t(a[n]),n++;for(;m<=b;){let _=j(s,d[b+1]);C(_,i[m]),d[m++]=_}for(;n<=h;){let _=a[n++];_!==null&&_t(_)}return this.ut=o,$t(s,d),T}});var Mr=bt(class extends D{constructor(){super(...arguments),this.key=u}render(s,t){return this.key=s,t}update(s,[t,e]){return t!==this.key&&($t(s),this.key=t),e}});var Ts={playing:"Playing",paused:"Paused",buffering:"Buffering"};function Os(s,t=[]){if(!s)return"Unavailable";let e=t.find(o=>o.source===s);if(e)return e.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...a]=s.split(":"),i=a.join(":");return r==="line-in"&&i?`Input ${i}`:r==="player"&&i?`Network player ${i}`:r==="chime"&&i?`Chime ${i}`:r==="soloist"&&i?"Spotify":s}var te=class extends g{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:ke(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=l`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:Mr(t.artwork,l`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,e=this.inputs??[];return l`
      ${t?l`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?l`<p data-artist>${t.artist}</p>`:u}
              ${t.album?l`<p data-album>${t.album}</p>`:u}
              <p data-state>${Ts[t.state]??"Unavailable"}</p>
            </div>
          </div>`:u}
      <p class="row" data-source=${this.source??""}>Source: ${Os(this.source,e)}</p>
      ${this.pick&&e.length>0?l`<ul aria-label="Inputs for ${this.name}">
            ${e.map(r=>l`<li data-input=${r.id}>
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
    `}};customElements.define("chorus-playing",te);var Cs=s=>`${Math.round(s/10)}%`,ee=class extends g{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let r=t.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(we(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(tt(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(i=>i.id)),r=t.defined??[],a=new Set(r.map(i=>i.id));return[...r.map(i=>({...i,playing:e.has(i.id)})),...t.rooms.filter(i=>!a.has(i.id)).map(i=>({...i,playing:!0}))]}render(){let t=this.group;if(!t)return u;let e=t.volume===null?"":Cs(this._dragged??t.volume);return l`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?u:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(r=>l`<li data-member=${r.id} data-playing=${String(r.playing)}>
              <span>${r.name}</span>
              ${r.playing?l`<button
                    type="button"
                    aria-label="Remove ${r.name} from ${t.name}"
                    @click=${()=>this._onRemove(r)}
                  >
                    Remove
                  </button>`:l`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.source?l`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${t.kind==="live"||t.active===!0}
          ></chorus-playing>`:u}
      ${t.kind==="saved"&&!t.active?l`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:u}
      ${t.volume===null?u:l`<div class="row">
            <label for="volume">Group volume</label>
            <input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Group volume for ${t.name}"
              aria-valuetext=${e}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />
            <span class="figure" data-volume>${e}</span>
          </div>`}
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:u}</p>
    `}};customElements.define("chorus-group-card",ee);var re=class extends g{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return l`
      ${this.groups!==null&&t.length===0?l`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:u}
      <ul>
        ${yt(t,r=>r.id,r=>l`<li
              data-group=${r.id}
              data-drop="group"
              data-drop-id=${r.id}
              ?data-over=${e?.kind==="group"&&e.id===r.id}
            >
              <chorus-group-card
                .group=${r}
                .inputs=${this.inputs}
                .refusal=${this.refusals[r.id]??""}
              ></chorus-group-card>
            </li>`)}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${e?.kind==="alone"}>
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:u}
      </p>
    `}};customElements.define("chorus-groups",re);var Pr=Object.freeze(["phone","desktop"]),Ns=48,Rs=`(min-width: ${Ns}em)`;function Lr(s,t=globalThis){if(typeof t?.matchMedia!="function")return s("phone"),()=>{};let e=t.matchMedia(Rs),r=()=>s(e.matches?"desktop":"phone");return e.addEventListener("change",r),r(),()=>e.removeEventListener("change",r)}var se=s=>`limits:${s}`,Ir={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},zs=Object.freeze({days:A,start:"22:00",end:"07:00",limit:250}),I=s=>`${Math.round(s/10)}%`,Ur=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),Ms=({days:s,start:t,end:e,limit:r})=>({days:s,start:t,end:e,limit:r}),ae=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...zs},this._held=new Set,this._asked=null,this._unanswered=0}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||e)&&(r.value=r.dataset.server)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:se(this.room.id),body:t,done:e},bubbles:!0,composed:!0}))}_askWindows(t){let e=this.room.id,r=(this._asked?.room===e?this._asked.windows:this.room.limits.windows).map(Ms);t(r),this._asked={room:e,windows:r},this._unanswered+=1,this._ask(xe(e,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onFocus(t){this._held.add(t.target.dataset.key)}_onBlur(t){let{key:e,server:r}=t.target.dataset;this._held.delete(e),this._release(e),t.target.value=r}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.key]:Number(t.target.value)}}_onLimitChange(t){this._release(t.target.dataset.key),this._ask(Ae(this.room.id,Number(t.target.value)))}_onEnabled(){this._ask(Ee(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(t){let e=Number(t.target.dataset.window),r=Number(t.target.value);this._release(t.target.dataset.key),this._askWindows(a=>{a[e]={...a[e],limit:r}})}_onWindowTime(t){let{window:e,edge:r,server:a}=t.target.dataset,i=t.target.value;if(!Ur(i)){t.target.value=a;return}i!==a&&this._askWindows(o=>{o[Number(e)]={...o[Number(e)],[r]:i}})}_onWindowDay(t){let{window:e,day:r}=t.currentTarget.dataset;this._askWindows(a=>{let i=a[Number(e)],o=i.days.includes(r)?i.days.filter(c=>c!==r):[...i.days,r];a[Number(e)]={...i,days:o}})}_onRemove(t){let e=Number(t.currentTarget.dataset.window);this._askWindows(r=>r.splice(e,1))}_onDraftDay(t){let e=t.currentTarget.dataset.day,r=this._draft.days.includes(e)?this._draft.days.filter(a=>a!==e):A.filter(a=>a===e||this._draft.days.includes(a));this._draft={...this._draft,days:r}}_onDraftTime(t){let e=t.target.dataset.edge;if(!Ur(t.target.value)){t.target.value=this._draft[e];return}this._draft={...this._draft,[e]:t.target.value}}_onDraftLimit(t){this._draft={...this._draft,limit:Number(t.target.value)}}_onAdd(){this._askWindows(t=>t.push({...this._draft}))}_days(t,e,r,a){let i=this.room;return l`
      <div class="row" role="group" aria-label="Days of ${e} for ${i.name}">
        ${A.map(o=>l`<button
              type="button"
              data-day=${o}
              data-window=${a??u}
              aria-label="${Ir[o][1]}, ${e} for ${i.name}"
              aria-pressed=${t.includes(o)?"true":"false"}
              @click=${r}
            >
              ${Ir[o][0]}
            </button>`)}
      </div>
    `}_window(t,e,r){let a=this.room,i=`window ${e+1}`,o=a.limits.quietEnabled!==!1,c=t.active?o?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return l`<li data-window=${e}><p data-value="active">Unavailable</p></li>`;let d=`window-${e}`;return l`
      <li data-window=${e} ?data-active=${t.active}>
        <div class="row">
          <strong>Window ${e+1}</strong>
          <span data-value="active" ?data-active=${t.active&&o}>${c}</span>
        </div>
        ${this._days(t.days,i,this._onWindowDay,e)}
        <div class="row">
          <label for="${d}-start">From</label>
          <input
            id="${d}-start"
            type="time"
            data-key="${d}-start"
            data-window=${e}
            data-edge="start"
            data-server=${t.start}
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
            data-window=${e}
            data-edge="end"
            data-server=${t.end}
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
            data-window=${e}
            data-server=${t.limit}
            aria-label="Limit of ${i} for ${a.name}"
            aria-valuetext=${I(this._dragged[`${d}-limit`]??t.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${I(this._dragged[`${d}-limit`]??t.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${e} aria-label="Remove ${i} for ${a.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(t){let e=this.room;if(t>=Tt)return l`<p data-full>A room has at most ${Tt} windows. Remove one to add another.</p>`;let r=this._draft,a="the new window";return l`
      <div class="draft" data-draft>
        ${this._days(r.days,a,this._onDraftDay)}
        <div class="row">
          <label for="draft-start">From</label>
          <input
            id="draft-start"
            type="time"
            data-edge="start"
            .value=${r.start}
            aria-label="Start of ${a} for ${e.name}"
            @change=${this._onDraftTime}
          />
          <label for="draft-end">Until</label>
          <input
            id="draft-end"
            type="time"
            data-edge="end"
            .value=${r.end}
            aria-label="End of ${a} for ${e.name}"
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
            aria-label="Limit of ${a} for ${e.name}"
            aria-valuetext=${I(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${I(r.limit)}</span>
        </div>
        <div class="row">
          <button
            type="button"
            aria-label="Add window for ${e.name}"
            ?disabled=${r.days.length===0}
            @click=${this._onAdd}
          >
            Add window
          </button>
          <p>${r.days.length===0?"A window starts on at least one day.":"Nothing is sent until it is added."}</p>
        </div>
      </div>
    `}render(){let t=this.room;if(!t)return l`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let{limit:e,effectiveLimit:r,quietEnabled:a,windows:i}=t.limits,o=i.every(p=>p.start&&p.end&&p.limit!==null&&p.days.length>0),c=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:u,d=e===null?"Unavailable":I(this._dragged.limit??e);return l`
      <h2>Volume limits of ${t.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${e===null?u:l`<input
              id="limit"
              type="range"
              min="0"
              max="1000"
              step="1"
              data-key="limit"
              data-server=${e}
              aria-label="Volume limit for ${t.name}"
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
        <span class="figure" data-value="effective">${r===null?"Unavailable":I(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${t.volume===null?"Unavailable":I(t.volume)}</span>
      </div>
      <h3>Quiet hours</h3>
      <div class="row">
        <button
          type="button"
          aria-label="Quiet hours for ${t.name}"
          aria-pressed=${a===!0?"true":"false"}
          ?disabled=${a===null}
          @click=${this._onEnabled}
        >
          Quiet hours
        </button>
        <span data-value="enabled">${a===null?"Unavailable":a?"On":"Off"}</span>
        <p>Off, no window caps the room, and every window is kept.</p>
      </div>
      ${i.length===0?l`<p data-none>This room has no quiet-hours window.</p>`:u}
      <ol aria-label="Quiet-hours windows of ${t.name}">
        ${i.map((p,f)=>this._window(p,f,o))}
      </ol>
      ${o?l`<h3>Add a window</h3>
            ${this._adding(i.length)}`:l`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||u}>${c}</p>
    `}};customElements.define("chorus-room-limits",ae);var ie="room-limits";w({id:ie,path:"rooms/:room/limits",title:({room:s},t)=>`Volume limits of ${F(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>l`
    <chorus-room-limits
      .room=${F(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[se(s)]??""}
      .refusalField=${r[se(s)]??""}
    ></chorus-room-limits>
  `});var Dr=Object.freeze(["app","kiosk"]),oe="chorus.kiosk",Fr="1";function Ps(s){let t=new URLSearchParams(s).get("kiosk");return t===null?null:t==="0"||t==="false"?"app":"kiosk"}function jr(s,t){let e=Ps(s);try{if(e==="kiosk")t?.setItem(oe,Fr);else if(e==="app")t?.removeItem(oe);else return t?.getItem(oe)===Fr?"kiosk":"app"}catch{}return e??"app"}function Hr(s=globalThis){try{return s.localStorage??null}catch{return null}}var ne=s=>`sound:${s}`,Wr=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],Ls=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],Is=s=>`${s>0?"+":""}${s} dB`,le=class extends g{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(t){return this.renderRoot.querySelector(`input[data-field="${t}"]`)}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of Wr){let a=this._slider(r),i=this.room.sound[r];!a||i===null||(!this._held.has(r)||e)&&(a.value=String(i))}}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:ne(this.room.id),body:Se(this.room.id,{[t]:e})},bubbles:!0,composed:!0}))}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onSliderFocus(t){this._held.add(t.target.dataset.field)}_onSliderBlur(t){let e=t.target.dataset.field;this._held.delete(e),this._release(e);let r=this.room?.sound[e];r!=null&&(t.target.value=String(r))}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.field]:Number(t.target.value)}}_onSliderChange(t){let e=t.target.dataset.field;this._release(e),this._ask(e,Number(t.target.value))}_onSwitch(t){let e=t.currentTarget.dataset.field;this._ask(e,!this.room.sound[e])}_tone({field:t,name:e}){let r=this.room,a=r.sound[t],i=a===null?"Unavailable":Is(this._dragged[t]??a);return l`
      <div class="row">
        <label for=${t}>${e}</label>
        ${a===null?u:l`<input
              id=${t}
              data-field=${t}
              type="range"
              min=${J.min}
              max=${J.max}
              step="1"
              aria-label="${e} for ${r.name}"
              aria-valuetext=${i}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${t}>${i}</span>
      </div>
    `}_switch({field:t,name:e,says:r}){let a=this.room,i=a.sound[t];return l`
      <div class="row">
        <button
          type="button"
          data-field=${t}
          aria-label="${e} for ${a.name}"
          aria-pressed=${i===!0?"true":"false"}
          ?disabled=${i===null}
          @click=${this._onSwitch}
        >
          ${e}
        </button>
        <span data-value=${t}>${i===null?"Unavailable":i?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let t=this.room;if(!t)return l`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let e=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:u;return l`
      <h2>Sound of ${t.name}</h2>
      ${Wr.map(r=>this._tone(r))} ${Ls.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||u}>${e}</p>
    `}};customElements.define("chorus-room-sound",le);var de="room-sound";w({id:de,path:"rooms/:room/sound",title:({room:s},t)=>`Sound of ${F(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>l`
    <chorus-room-sound
      .room=${F(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[ne(s)]??""}
      .refusalField=${r[ne(s)]??""}
    ></chorus-room-sound>
  `});var Us={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Fs=s=>`${Math.round(s/10)}%`,ue=class extends g{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let a=t.has("refusal")&&!!this.refusal;a&&(this._dragged=null),(!this._sliderHeld||a)&&(r.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(_e(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let r=Sr(e);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(ye(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return u;let e=t.volume===null?"Unavailable":Fs(this._dragged??t.volume);return l`
      <div class="head">
        <h2>${t.name}</h2>
        <button
          type="button"
          class="handle"
          data-drag-room=${t.id}
          aria-label="Move ${t.name}"
          title="Drag onto a room or a group, or press to choose from the list"
          @click=${this._onHandle}
        >
          Move
        </button>
        <a href=${S(de,{room:t.id})} data-route aria-label="Sound for ${t.name}">Sound</a>
        <a href=${S(ie,{room:t.id})} data-route aria-label="Limits for ${t.name}">Limits</a>
      </div>
      ${t.bond.length===0?u:l`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(r=>l`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${Us[r.role]??r.role}: ${r.name}
                  </li>`)}
            </ul>
          `}
      ${t.source?l`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:u}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?u:l`<input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Volume for ${t.name}"
              aria-valuetext=${e}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-volume>${e}</span>
      </div>
      <div class="row">
        <button
          type="button"
          aria-label="Mute ${t.name}"
          aria-pressed=${t.muted===!0?"true":"false"}
          ?disabled=${t.muted===null}
          @click=${this._onMute}
        >
          Mute
        </button>
        <span data-mute>${t.muted===null?"Unavailable":t.muted?"Muted":"Not muted"}</span>
      </div>
      <div class="row">
        <label for="place">Plays with</label>
        <select id="place" aria-label="Group for ${t.name}" @change=${this._onPlace}>
          ${this.places.map(r=>l`<option value=${r.value} ?selected=${r.value===this.place}>${r.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:u}</p>
    `}};customElements.define("chorus-room-card",ue);var ce=class extends g{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],r=this.over;return l`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?l`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:u}
      <ul>
        ${yt(t??[],a=>a.id,a=>l`<li
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
                .places=${xr(a,t,e)}
                .place=${Ar(a,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",ce);var he="chorus-setup-",pe=6,Ds=12,js=Object.freeze({form:"GET /",takes:"POST /join",title:"chorus speaker setup"}),Hs=Object.freeze([{id:"power",title:"Switch the speaker on",text:[`A Wi-Fi speaker that knows no network raises a Wi-Fi access point of its own, named ${he} and ${pe} characters (${he}<${pe} characters>).`,`Its setup secret is ${Ds} characters and is the access point's password. The speaker prints it, and the address of its join page, on its serial console when the access point comes up.`]},{id:"access-point",title:"Join the speaker's access point",text:[`In this phone's Wi-Fi settings, join the network ${he}<${pe} characters> with the setup secret as its password. Accept that it has no internet.`,"The phone is then off the house's network, and this page cannot reach the chorus server until it is back. That is expected. Leave this page open."]},{id:"join-page",title:"Open the speaker's join page",text:[`In the phone's browser, open the address the speaker printed (http://<address>/). The speaker serves the page itself, on its access point: it is titled "${js.title}" and is a form with two fields.`,"Type the house network's name and its passphrase into that page, and press Join. They go to the speaker and nowhere else: this app never asks for them. The network has to be on 2.4 GHz and have a passphrase; the speaker refuses an open network.",'The page answers "Received". If the join fails the access point stays up: join it again and load the page again, and it says why above the form (auth-error for a wrong passphrase, network-not-found for a name it cannot see).']},{id:"return",title:"Come back to the house's network",text:["The speaker takes its access point down and joins the house's network. The phone goes back to the house's network on its own, or join it again in the Wi-Fi settings.","When the speaker reaches the chorus server it is adopted, and this page says so by itself. There is nothing to press."]}]);function Ws(s,t){let e=new Set(s??[]);return(t??[]).filter(r=>!e.has(r.id))}var fe="chorus-speaker-setup";function Bs(s){try{let t=JSON.parse(s?.getItem(fe)??"null");return Array.isArray(t)&&t.every(e=>typeof e=="string")?t:null}catch{return null}}function me(s,t){try{t===null?s?.removeItem(fe):s?.setItem(fe,JSON.stringify(t))}catch{}}var Js=()=>{try{return globalThis.sessionStorage??null}catch{return null}},ge=class extends g{static properties={speakers:{attribute:!1},status:{type:String},back:{type:String},storage:{attribute:!1},_baseline:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.status="connecting",this.back="#/",this.storage=Js(),this._baseline=null}willUpdate(){!this.isConnected||this._baseline!==null||!Array.isArray(this.speakers)||(this._baseline=Bs(this.storage)??this.speakers.map(t=>t.id),me(this.storage,this._baseline))}disconnectedCallback(){super.disconnectedCallback(),me(this.storage,null),this._baseline=null}_onAgain(){Array.isArray(this.speakers)&&(this._baseline=this.speakers.map(t=>t.id),me(this.storage,this._baseline))}_status(t){return t.length>0?u:this.status==="lost"||this.status==="signed-out"?l`<p role="status" data-away>
        ${this.status==="signed-out"?"Signed out of the chorus server: sign in again to go on.":"This page cannot reach the chorus server now. That is expected while the phone is on the speaker's access point: it goes on by itself when the phone is back on the house's network."}
      </p>`:this._baseline===null?l`<p role="status" data-waiting>Reading this server's speakers.</p>`:l`<p role="status" data-waiting>
      Waiting for a new speaker. This page goes on by itself when one is adopted.
    </p>`}_done(t){return t.length===0?u:l`
      <div data-done role="status">
        ${t.map(e=>l`<p data-arrived=${e.id}>${e.name} (${e.id}) joined and was adopted.</p>`)}
        <p>It has no name of its own and is in no room yet.</p>
        <div class="row">
          <a href=${this.back} data-route aria-label="Name the new speaker and give it a room">Name it and give it a room</a>
          <button type="button" aria-label="Set up another speaker" @click=${this._onAgain}>Set up another</button>
        </div>
      </div>
    `}render(){let t=this._baseline===null?[]:Ws(this._baseline,this.speakers);return l`
      <h2>Set up a Wi-Fi speaker</h2>
      <p>
        A compact Wi-Fi speaker learns the house's network from a phone, on a page the speaker serves itself. This app
        says the steps and watches for the speaker; it never asks for the network's passphrase.
      </p>
      ${this._done(t)}
      <ol aria-label="Steps" ?data-complete=${t.length>0}>
        ${Hs.map((e,r)=>l`
            <li data-step=${e.id}>
              <h3>${r+1}. ${e.title}</h3>
              ${e.text.map(a=>l`<p>${a}</p>`)}
            </li>
          `)}
      </ol>
      ${this._status(t)}
    `}};customElements.define("chorus-speaker-setup",ge);var Br=s=>`speaker:${s}`,qs={wired:"Wired",wireless:"Wi-Fi"},ve=class extends g{static properties={speakers:{attribute:!1},keyChanges:{attribute:!1},rooms:{attribute:!1},refusals:{attribute:!1},setup:{type:String},_drafts:{state:!0},_forgetting:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.speakers=null,this.keyChanges=[],this.rooms=[],this.refusals={},this.setup="",this._drafts={},this._forgetting=null}_speaker(t){return(this.speakers??[]).find(e=>e.id===t)??null}willUpdate(t){if(!t.has("speakers"))return;let e=Object.entries(this._drafts).filter(([r,a])=>{let i=this._speaker(r);return i&&!(i.named&&i.name===a.trim())});e.length!==Object.keys(this._drafts).length&&(this._drafts=Object.fromEntries(e)),this._forgetting!==null&&!this._speaker(this._forgetting)&&(this._forgetting=null)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Br(t),body:e},bubbles:!0,composed:!0}))}_onDraft(t){this._drafts={...this._drafts,[t.target.dataset.speaker]:t.target.value}}_name(t){let e=this._speaker(t),r=(this._drafts[t]??e?.name??"").trim();!e||!r||e.named&&r===e.name||this._ask(t,ze(t,r))}_onName(t){this._name(t.currentTarget.dataset.speaker)}_onNameKey(t){t.key==="Enter"&&(t.preventDefault(),this._name(t.target.dataset.speaker))}_onRoom(t){let e=this._speaker(t.target.dataset.speaker),r=t.target.value,a=e?.room??"";t.target.value=a,!(!e||r===a)&&this._ask(e.id,Me(e.id,r||null))}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-speaker]")){let e=this._speaker(t.dataset.speaker)?.room??"";t.value!==e&&(t.value=e)}}_onForget(t){let{speaker:e,forget:r}=t.currentTarget.dataset;if(r==="ask"){this._forgetting=e;return}this._forgetting=null,r==="yes"&&this._ask(e,Pe(e))}_rooms(t){let e=this.rooms??[],r=t.room===null||e.some(a=>a.id===t.room);return l`
      <option value="" ?selected=${t.room===null}>No room</option>
      ${r?u:l`<option value=${t.room} selected>${t.room} (not on this server now)</option>`}
      ${e.map(a=>l`<option value=${a.id} ?selected=${t.room===a.id}>${a.name}</option>`)}
    `}_forget(t){let{id:e,name:r}=t;return this._forgetting!==e?l`
        <div class="row">
          <button type="button" data-speaker=${e} data-forget="ask" aria-label="Forget ${r}" @click=${this._onForget}>
            Forget
          </button>
        </div>
      `:l`
      <p data-forget-question>
        Forget ${r}? Its name, its room and its pinned key are removed. If it connects again it is adopted as a new
        speaker, whatever key it then offers.
      </p>
      <div class="row">
        <button type="button" data-speaker=${e} data-forget="yes" aria-label="Yes, forget ${r}" @click=${this._onForget}>
          Yes, forget it
        </button>
        <button type="button" data-speaker=${e} data-forget="no" aria-label="Keep ${r}" @click=${this._onForget}>
          Keep it
        </button>
      </div>
    `}_row(t){let{id:e,name:r}=t,a=this.refusals?.[Br(e)]??"",i=(this.keyChanges??[]).some(d=>d.id===e),o=this._drafts[e],c=(o??r).trim();return l`
      <li data-speaker=${e} ?data-new=${t.isNew}>
        <h3>${r}</h3>
        ${t.isNew?l`<p data-new-mark>New: adopted, not named and in no room yet.</p>`:u}
        ${i?l`<p data-key-changed>A session under this id offered another key and was refused (above).</p>`:u}
        <p data-id>${e}</p>
        <dl>
          <div>
            <dt>Now</dt>
            <dd data-value="present">${t.present?"Connected":"Not connected"}</dd>
          </div>
          <div>
            <dt>Link</dt>
            <dd data-value="link">${qs[t.link]??"Not reported"}</dd>
          </div>
          <div>
            <dt>Software</dt>
            <dd data-value="software">${t.software??"Not said yet"}</dd>
          </div>
          <div>
            <dt>Key</dt>
            <dd data-value="key">${t.key??"Not known"}</dd>
          </div>
        </dl>
        <div class="row">
          <label for="name-${e}">Name</label>
          <input
            id="name-${e}"
            type="text"
            autocomplete="off"
            data-speaker=${e}
            aria-label="Name of ${r}"
            .value=${o??r}
            @input=${this._onDraft}
            @keydown=${this._onNameKey}
          />
          <button
            type="button"
            data-speaker=${e}
            aria-label="Save the name of ${r}"
            ?disabled=${!c||t.named&&c===r}
            @click=${this._onName}
          >
            Save name
          </button>
        </div>
        <div class="row">
          <label for="room-${e}">Room</label>
          <select id="room-${e}" data-speaker=${e} aria-label="Room of ${r}" @change=${this._onRoom}>
            ${this._rooms(t)}
          </select>
        </div>
        ${this._forget(t)}
        <p role="alert">${a?`Refused: ${a}`:u}</p>
      </li>
    `}_keyChange(t){let e=this._speaker(t.id);return l`
      <li data-key-change=${t.id}>
        <h3>Refused: ${e?.name??t.id} offered a changed key</h3>
        <p role="alert">
          A session under the id <span data-id>${t.id}</span> offered a key that is not the one this id is pinned
          to. The server refused it, and the pinned key did not move.
        </p>
        <dl>
          <div>
            <dt>Pinned key</dt>
            <dd><code data-value="pinned">${t.pinned??"not known"}</code></dd>
          </div>
          <div>
            <dt>Offered key, refused</dt>
            <dd><code data-value="offered">${t.offered??"not known"}</code></dd>
          </div>
        </dl>
        <p>
          Nothing here accepts the offered key.
          ${e?`If you replaced or wiped this speaker yourself, forget ${e.name} below: its next session is then adopted as a new speaker. If you did not, something else is answering under its id.`:"This id is not among the speakers listed here, so nothing on this screen can forget it."}
        </p>
      </li>
    `}render(){if(this.speakers===null)return l`<p role="status" data-missing>Reading this server's speakers.</p>`;let t=this.keyChanges??[];return l`
      <h2>Speakers</h2>
      <p>A speaker is adopted when it first connects. Name it and put it in a room here.</p>
      ${t.length===0?u:l`<ul aria-label="Changed keys">
            ${t.map(e=>this._keyChange(e))}
          </ul>`}
      ${this.speakers.length===0?l`<p role="status" data-none>This server has adopted no speaker yet.</p>`:l`<ul aria-label="Adopted speakers">
            ${this.speakers.map(e=>this._row(e))}
          </ul>`}
      ${this.setup?l`<a href=${this.setup} data-route aria-label="Set up a Wi-Fi speaker">Set up a Wi-Fi speaker</a>`:u}
    `}};customElements.define("chorus-speakers",ve);var kt="speakers",Jr="speaker-setup";w({id:kt,path:"speakers",title:()=>"Speakers",render:(s,{view:t,refusals:e})=>l`
    <chorus-speakers
      .speakers=${t.state===null?null:Jt(t.state)}
      .keyChanges=${mr(t.state)}
      .rooms=${t.rooms.map(({id:r,name:a})=>({id:r,name:a}))}
      .refusals=${e}
      .setup=${S(Jr)}
    ></chorus-speakers>
  `});w({id:Jr,path:"speakers/setup",title:()=>"Set up a Wi-Fi speaker",render:(s,{view:t})=>l`
    <chorus-speaker-setup
      .speakers=${t.state===null?null:Jt(t.state)}
      .status=${t.status}
      .back=${S(kt)}
    ></chorus-speaker-setup>
  `});var be=class extends g{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=or(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",t=>this._onLink(t)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=sr({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!Z(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){Dr.includes(this.mode)||(this.mode="app"),Pr.includes(this.layout)||(this.layout="phone"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=Lr(t=>{this.layout=t}),this._unroute?.(),this._unroute=this._navigation.watch(t=>{t.address!==this._route.address&&(this._route=t)})}updated(t){if(!t.has("_route")||t.get("_route")===void 0)return;let e=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(e==="groups"?"section":"main");r&&(e&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!e}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""},this._refusalFields={...this._refusalFields,[t]:""};let r=await this.store.command(e);r.ok||(this._refusals={...this._refusals,[t]:r.refusal},this._refusalFields={...this._refusalFields,[t]:r.field??""})}_onCommand(t){let{subject:e,room:r,body:a,done:i}=t.detail;this._send(e??r,a).then(()=>i?.())}_move(t,e){let r=this._room(t),a=wr(r,e,this._groups);a&&this._send(t,a)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_onGo(t){let e=t.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=e,this._navigation.back();return}let r=this.renderRoot.querySelector(e==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(t){if(t.defaultPrevented||t.button>0||t.metaKey||t.ctrlKey||t.shiftKey||t.altKey)return;let e=t.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));e&&(t.preventDefault(),e.dataset.route==="back"?this._navigation.back():this._navigation.open(e.getAttribute("href")))}_screen(t){let e=Ht(t.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return l`
      <main
        aria-label=${e.title(t.params,this._view)}
        data-screen=${e.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${ft} data-route="back" aria-label="Back to rooms">Back</a>
        ${e.render(t.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?u:l`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return l`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()} ${this._route.screen===mt.screen?this._home():this._screen(this._route)}
    `}_home(){return l`
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
        <a class="more" href=${S(Zt)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${S(Yt)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <a class="more" href=${S(kt)} data-route aria-label="Speakers and their setup">Speakers</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",be);var Vs="sw.js";async function qr(s=globalThis.navigator){let t=s?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(Vs,{scope:"./",updateViaCache:"none"})}catch{return null}}function Vr({navigator:s=globalThis.navigator,document:t=globalThis.document}={}){let e=null;try{e=s?.wakeLock??null}catch{e=null}if(!e||typeof e.request!="function"||typeof t?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,a=null,i=!1,o=async d=>{try{await d.release()}catch{}},c=()=>{i||r||a||t.visibilityState!=="visible"||(a=(async()=>{try{let d=await e.request("screen");if(i){await o(d);return}r=d,d.addEventListener?.("release",()=>{r===d&&(r=null)})}catch{}finally{a=null}})())};return t.addEventListener("visibilitychange",c),c(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;a;)await a},stop:async()=>{for(i=!0,t.removeEventListener("visibilitychange",c);a;)await a;let d=r;r=null,d&&await o(d)}}}var wt=document.querySelector("chorus-app");if(wt){wt.mode=jr(window.location.search,Hr(window)),wt.mode==="kiosk"&&Vr();let s=gr(Le());wt.store=s,s.start()}qr();
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
