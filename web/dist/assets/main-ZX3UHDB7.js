function J(s){let t=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function he(s,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${J(t)}}`}function pe(s,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${t?"true":"false"}}`}function xt(s,t){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(t)}}`}function tt(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function me(s,t){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(t)}}`}function fe(s,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${J(t)}}`}var q=Object.freeze({min:-10,max:10}),Pr=["bass","treble"],Ir=["loudness","night","speech"];function ge(s,t={}){let e=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of Pr){if(t[r]===void 0)continue;let i=Math.min(q.max,Math.max(q.min,Math.round(Number(t[r])||0)));e+=`,"${r}":${i}`}for(let r of Ir)t[r]!==void 0&&(e+=`,"${r}":${t[r]?"true":"false"}`);return`${e}}`}function ve(s,t){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${J(t)}}`}var w=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),Et=8;function be(s,t=[]){let e=t.map(r=>{let i=w.filter(a=>(r.days??[]).includes(a));return`{"days":${JSON.stringify(i)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${J(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${e.join(",")}]}`}function $e(s,t){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${t?"true":"false"}}`}function _e(s,t,e,{stopOnStandby:r=!0,lowLatency:i=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(t)},"enabled":${e?"true":"false"}${r===!1?',"stop_on_standby":false':""}${i===!1?',"low_latency":false':""}}`}var et=600,rt=720,st=720,At=(s,t)=>Math.min(t,Math.max(0,Math.round(Number(s)||0)));function Ot({alarm:s,target:t,time:e,days:r=[],source:i,volume:a,rampS:o,durationMin:u,enabled:l}){let p=w.filter(f=>r.includes(f));return`{"v":2,"t":"alarm_set","alarm":${JSON.stringify(s)},"target":${JSON.stringify(t)},"time":${JSON.stringify(String(e))},"days":${JSON.stringify(p)},"source":${JSON.stringify(i)},"volume":${J(a)},"ramp_s":${At(o,et)},"duration_min":${At(u,rt)},"enabled":${l?"true":"false"}}`}function ye(s){return`{"v":2,"t":"alarm_delete","alarm":${JSON.stringify(s)}}`}function we(s){return`{"v":2,"t":"alarm_stop","alarm":${JSON.stringify(s)}}`}function Tt(s,t){return`{"v":2,"t":"sleep","target":${JSON.stringify(s)},"minutes":${At(t,st)}}`}function ke(s,t,e,r){return`{"v":2,"t":"source_store","id":${JSON.stringify(s)},"kind":${JSON.stringify(t)},"value":${JSON.stringify(e)},"name":${JSON.stringify(r)}}`}function Se(s){return`{"v":2,"t":"source_forget","id":${JSON.stringify(s)}}`}function Ct(s,t,e=""){let r=5381;for(let i of String(e))r=(Math.imul(r,33)^i.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(t)}${e?`#${r.toString(36)}`:""}`}function St(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var ce="Signed out";async function Dr(s){let t="";try{t=(await s.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail){let r=typeof e.field=="string"&&e.field?{field:e.field}:{};return{refusal:e.detail,...r}}}catch{}return{refusal:t||`the server answered ${s.status}`}}var Ur={set:(s,t)=>globalThis.setTimeout(s,t),clear:s=>globalThis.clearTimeout(s)};function Ae({fetch:s=globalThis.fetch.bind(globalThis),base:t="../",timers:e=Ur}={}){async function r(){let u=await s(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(St(u))throw Object.assign(new Error(ce),{signedOut:!0});if(!u.ok)throw new Error(`the server answered ${u.status}`);return u.json()}async function i(u){let l;try{l=await s(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:u,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(St(l))return{ok:!1,refusal:ce,signedOut:!0};if(!l.ok)return{ok:!1,...await Dr(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function a({onState:u,onStatus:l=()=>{}}){let p=!1,f=null,n=null,h=null,m=()=>{h!==null&&e.clear(h),h=null},v=()=>{m(),h=e.set(()=>f?.abort(),4e4)},_=y=>{let x=y.split(`
`).filter(B=>B.startsWith("data:")).map(B=>B.slice(5).replace(/^ /,"")).join(`
`);if(!x)return;let H;try{H=JSON.parse(x)}catch{return}l("live"),u(H)};async function g(){f=new AbortController,v();let y=!1;try{let x=await s(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:f.signal});if(y=St(x),!x.ok||!x.body)throw new Error(`the server answered ${x.status}`);let H=x.body.getReader();f.signal.addEventListener("abort",()=>H.cancel().catch(()=>{}));let B=new TextDecoder,W="";for(;;){let{done:zr,value:Lr}=await H.read();if(zr||p||f.signal.aborted)break;v(),W+=B.decode(Lr,{stream:!0}).replace(/\r\n?/g,`
`);let kt;for(;(kt=W.indexOf(`

`))!==-1;)_(W.slice(0,kt)),W=W.slice(kt+2)}}catch{}m(),!p&&(l(y?"signed-out":"lost"),n=e.set(()=>{n=null,g()},1e3))}return g(),()=>{p=!0,m(),n!==null&&e.clear(n),f?.abort()}}return{state:r,command:i,events:a,artwork:(u,l)=>Ct(t,u,l)}}var it=globalThis,at=it.ShadowRoot&&(it.ShadyCSS===void 0||it.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,Nt=Symbol(),xe=new WeakMap,V=class{constructor(t,e,r){if(this._$cssResult$=!0,r!==Nt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(at&&t===void 0){let r=e!==void 0&&e.length===1;r&&(t=xe.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),r&&xe.set(e,t))}return t}toString(){return this.cssText}},Ee=s=>new V(typeof s=="string"?s:s+"",void 0,Nt),$=(s,...t)=>{let e=s.length===1?s[0]:t.reduce((r,i,a)=>r+(o=>{if(o._$cssResult$===!0)return o.cssText;if(typeof o=="number")return o;throw Error("Value passed to 'css' function must be a 'css' function result: "+o+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(i)+s[a+1],s[0]);return new V(e,s,Nt)},Oe=(s,t)=>{if(at)s.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let r=document.createElement("style"),i=it.litNonce;i!==void 0&&r.setAttribute("nonce",i),r.textContent=e.cssText,s.appendChild(r)}},Rt=at?s=>s:s=>s instanceof CSSStyleSheet?(t=>{let e="";for(let r of t.cssRules)e+=r.cssText;return Ee(e)})(s):s;var{is:Fr,defineProperty:jr,getOwnPropertyDescriptor:Hr,getOwnPropertyNames:Br,getOwnPropertySymbols:Wr,getPrototypeOf:qr}=Object,ot=globalThis,Te=ot.trustedTypes,Jr=Te?Te.emptyScript:"",Vr=ot.reactiveElementPolyfillSupport,G=(s,t)=>s,Mt={toAttribute(s,t){switch(t){case Boolean:s=s?Jr:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,t){let e=s;switch(t){case Boolean:e=s!==null;break;case Number:e=s===null?null:Number(s);break;case Object:case Array:try{e=JSON.parse(s)}catch{e=null}}return e}},Ne=(s,t)=>!Fr(s,t),Ce={attribute:!0,type:String,converter:Mt,reflect:!1,useDefault:!1,hasChanged:Ne};Symbol.metadata??=Symbol("metadata"),ot.litPropertyMetadata??=new WeakMap;var k=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=Ce){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let r=Symbol(),i=this.getPropertyDescriptor(t,r,e);i!==void 0&&jr(this.prototype,t,i)}}static getPropertyDescriptor(t,e,r){let{get:i,set:a}=Hr(this.prototype,t)??{get(){return this[e]},set(o){this[e]=o}};return{get:i,set(o){let u=i?.call(this);a?.call(this,o),this.requestUpdate(t,u,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??Ce}static _$Ei(){if(this.hasOwnProperty(G("elementProperties")))return;let t=qr(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(G("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(G("properties"))){let e=this.properties,r=[...Br(e),...Wr(e)];for(let i of r)this.createProperty(i,e[i])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[r,i]of e)this.elementProperties.set(r,i)}this._$Eh=new Map;for(let[e,r]of this.elementProperties){let i=this._$Eu(e,r);i!==void 0&&this._$Eh.set(i,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let r=new Set(t.flat(1/0).reverse());for(let i of r)e.unshift(Rt(i))}else t!==void 0&&e.push(Rt(t));return e}static _$Eu(t,e){let r=e.attribute;return r===!1?void 0:typeof r=="string"?r:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let r of e.keys())this.hasOwnProperty(r)&&(t.set(r,this[r]),delete this[r]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Oe(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,r){this._$AK(t,r)}_$ET(t,e){let r=this.constructor.elementProperties.get(t),i=this.constructor._$Eu(t,r);if(i!==void 0&&r.reflect===!0){let a=(r.converter?.toAttribute!==void 0?r.converter:Mt).toAttribute(e,r.type);this._$Em=t,a==null?this.removeAttribute(i):this.setAttribute(i,a),this._$Em=null}}_$AK(t,e){let r=this.constructor,i=r._$Eh.get(t);if(i!==void 0&&this._$Em!==i){let a=r.getPropertyOptions(i),o=typeof a.converter=="function"?{fromAttribute:a.converter}:a.converter?.fromAttribute!==void 0?a.converter:Mt;this._$Em=i;let u=o.fromAttribute(e,a.type);this[i]=u??this._$Ej?.get(i)??u,this._$Em=null}}requestUpdate(t,e,r,i=!1,a){if(t!==void 0){let o=this.constructor;if(i===!1&&(a=this[t]),r??=o.getPropertyOptions(t),!((r.hasChanged??Ne)(a,e)||r.useDefault&&r.reflect&&a===this._$Ej?.get(t)&&!this.hasAttribute(o._$Eu(t,r))))return;this.C(t,e,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:r,reflect:i,wrapped:a},o){r&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,o??e??this[t]),a!==!0||o!==void 0)||(this._$AL.has(t)||(this.hasUpdated||r||(e=void 0),this._$AL.set(t,e)),i===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[i,a]of this._$Ep)this[i]=a;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[i,a]of r){let{wrapped:o}=a,u=this[i];o!==!0||this._$AL.has(i)||u===void 0||this.C(i,void 0,a,u)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(e)):this._$EM()}catch(r){throw t=!1,this._$EM(),r}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};k.elementStyles=[],k.shadowRootOptions={mode:"open"},k[G("elementProperties")]=new Map,k[G("finalized")]=new Map,Vr?.({ReactiveElement:k}),(ot.reactiveElementVersions??=[]).push("2.1.2");var Lt=globalThis,Re=s=>s,nt=Lt.trustedTypes,Me=nt?nt.createPolicy("lit-html",{createHTML:s=>s}):void 0,Pt="$lit$",S=`lit$${Math.random().toFixed(9).slice(2)}$`,It="?"+S,Gr=`<${It}>`,N=document,Y=()=>N.createComment(""),X=s=>s===null||typeof s!="object"&&typeof s!="function",Dt=Array.isArray,Ue=s=>Dt(s)||typeof s?.[Symbol.iterator]=="function",zt=`[ 	
\f\r]`,K=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,ze=/-->/g,Le=/>/g,T=RegExp(`>|${zt}(?:([^\\s"'>=/]+)(${zt}*=${zt}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Pe=/'/g,Ie=/"/g,Fe=/^(?:script|style|textarea|title)$/i,Ut=s=>(t,...e)=>({_$litType$:s,strings:t,values:e}),d=Ut(1),Rs=Ut(2),Ms=Ut(3),A=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),De=new WeakMap,C=N.createTreeWalker(N,129);function je(s,t){if(!Dt(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return Me!==void 0?Me.createHTML(t):t}var He=(s,t)=>{let e=s.length-1,r=[],i,a=t===2?"<svg>":t===3?"<math>":"",o=K;for(let u=0;u<e;u++){let l=s[u],p,f,n=-1,h=0;for(;h<l.length&&(o.lastIndex=h,f=o.exec(l),f!==null);)h=o.lastIndex,o===K?f[1]==="!--"?o=ze:f[1]!==void 0?o=Le:f[2]!==void 0?(Fe.test(f[2])&&(i=RegExp("</"+f[2],"g")),o=T):f[3]!==void 0&&(o=T):o===T?f[0]===">"?(o=i??K,n=-1):f[1]===void 0?n=-2:(n=o.lastIndex-f[2].length,p=f[1],o=f[3]===void 0?T:f[3]==='"'?Ie:Pe):o===Ie||o===Pe?o=T:o===ze||o===Le?o=K:(o=T,i=void 0);let m=o===T&&s[u+1].startsWith("/>")?" ":"";a+=o===K?l+Gr:n>=0?(r.push(p),l.slice(0,n)+Pt+l.slice(n)+S+m):l+S+(n===-2?u:m)}return[je(s,a+(s[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),r]},Q=class s{constructor({strings:t,_$litType$:e},r){let i;this.parts=[];let a=0,o=0,u=t.length-1,l=this.parts,[p,f]=He(t,e);if(this.el=s.createElement(p,r),C.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(i=C.nextNode())!==null&&l.length<u;){if(i.nodeType===1){if(i.hasAttributes())for(let n of i.getAttributeNames())if(n.endsWith(Pt)){let h=f[o++],m=i.getAttribute(n).split(S),v=/([.?@])?(.*)/.exec(h);l.push({type:1,index:a,name:v[2],strings:m,ctor:v[1]==="."?dt:v[1]==="?"?ut:v[1]==="@"?ct:M}),i.removeAttribute(n)}else n.startsWith(S)&&(l.push({type:6,index:a}),i.removeAttribute(n));if(Fe.test(i.tagName)){let n=i.textContent.split(S),h=n.length-1;if(h>0){i.textContent=nt?nt.emptyScript:"";for(let m=0;m<h;m++)i.append(n[m],Y()),C.nextNode(),l.push({type:2,index:++a});i.append(n[h],Y())}}}else if(i.nodeType===8)if(i.data===It)l.push({type:2,index:a});else{let n=-1;for(;(n=i.data.indexOf(S,n+1))!==-1;)l.push({type:7,index:a}),n+=S.length-1}a++}}static createElement(t,e){let r=N.createElement("template");return r.innerHTML=t,r}};function R(s,t,e=s,r){if(t===A)return t;let i=r!==void 0?e._$Co?.[r]:e._$Cl,a=X(t)?void 0:t._$litDirective$;return i?.constructor!==a&&(i?._$AO?.(!1),a===void 0?i=void 0:(i=new a(s),i._$AT(s,e,r)),r!==void 0?(e._$Co??=[])[r]=i:e._$Cl=i),i!==void 0&&(t=R(s,i._$AS(s,t.values),i,r)),t}var lt=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:r}=this._$AD,i=(t?.creationScope??N).importNode(e,!0);C.currentNode=i;let a=C.nextNode(),o=0,u=0,l=r[0];for(;l!==void 0;){if(o===l.index){let p;l.type===2?p=new I(a,a.nextSibling,this,t):l.type===1?p=new l.ctor(a,l.name,l.strings,this,t):l.type===6&&(p=new ht(a,this,t)),this._$AV.push(p),l=r[++u]}o!==l?.index&&(a=C.nextNode(),o++)}return C.currentNode=N,i}p(t){let e=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(t,r,e),e+=r.strings.length-2):r._$AI(t[e])),e++}},I=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,r,i){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=r,this.options=i,this._$Cv=i?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=R(this,t,e),X(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==A&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):Ue(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&X(this._$AH)?this._$AA.nextSibling.data=t:this.T(N.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:r}=t,i=typeof r=="number"?this._$AC(t):(r.el===void 0&&(r.el=Q.createElement(je(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===i)this._$AH.p(e);else{let a=new lt(i,this),o=a.u(this.options);a.p(e),this.T(o),this._$AH=a}}_$AC(t){let e=De.get(t.strings);return e===void 0&&De.set(t.strings,e=new Q(t)),e}k(t){Dt(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,r,i=0;for(let a of t)i===e.length?e.push(r=new s(this.O(Y()),this.O(Y()),this,this.options)):r=e[i],r._$AI(a),i++;i<e.length&&(this._$AR(r&&r._$AB.nextSibling,i),e.length=i)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let r=Re(t).nextSibling;Re(t).remove(),t=r}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},M=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,r,i,a){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=e,this._$AM=i,this.options=a,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(t,e=this,r,i){let a=this.strings,o=!1;if(a===void 0)t=R(this,t,e,0),o=!X(t)||t!==this._$AH&&t!==A,o&&(this._$AH=t);else{let u=t,l,p;for(t=a[0],l=0;l<a.length-1;l++)p=R(this,u[r+l],e,l),p===A&&(p=this._$AH[l]),o||=!X(p)||p!==this._$AH[l],p===c?t=c:t!==c&&(t+=(p??"")+a[l+1]),this._$AH[l]=p}o&&!i&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},dt=class extends M{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},ut=class extends M{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},ct=class extends M{constructor(t,e,r,i,a){super(t,e,r,i,a),this.type=5}_$AI(t,e=this){if((t=R(this,t,e,0)??c)===A)return;let r=this._$AH,i=t===c&&r!==c||t.capture!==r.capture||t.once!==r.once||t.passive!==r.passive,a=t!==c&&(r===c||i);i&&this.element.removeEventListener(this.name,this,r),a&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},ht=class{constructor(t,e,r){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(t){R(this,t)}},Be={M:Pt,P:S,A:It,C:1,L:He,R:lt,D:Ue,V:R,I,H:M,N:ut,U:ct,B:dt,F:ht},Kr=Lt.litHtmlPolyfillSupport;Kr?.(Q,I),(Lt.litHtmlVersions??=[]).push("3.3.3");var We=(s,t,e)=>{let r=e?.renderBefore??t,i=r._$litPart$;if(i===void 0){let a=e?.renderBefore??null;r._$litPart$=i=new I(t.insertBefore(Y(),a),a,void 0,e??{})}return i._$AI(s),i};var Ft=globalThis,b=class extends k{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=We(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return A}};b._$litElement$=!0,b.finalized=!0,Ft.litElementHydrateSupport?.({LitElement:b});var Yr=Ft.litElementPolyfillSupport;Yr?.({LitElement:b});(Ft.litElementVersions??=[]).push("4.2.2");function Xr(s,t,e){let r=s.elementFromPoint?.(t,e)??null;for(;r?.shadowRoot?.elementFromPoint;){let i=r.shadowRoot.elementFromPoint(t,e);if(!i||i===r)break;r=i}return r}function Qr(s){for(let t=s;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var qe=(s,t,e)=>Qr(Xr(s,t,e));function Je({root:s=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:r=()=>{}}={}){let i=null,a=()=>{let{handle:n,pointerId:h}=i;n.removeEventListener("pointermove",o),n.removeEventListener("pointerup",u),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),s.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(h)}catch{}i=null};function o(n){if(!(!i||n.pointerId!==i.pointerId)){if(!i.moving){if(Math.hypot(n.clientX-i.x,n.clientY-i.y)<8)return;i.moving=!0,t(i.room)}n.preventDefault(),e(qe(s,n.clientX,n.clientY))}}function u(n){if(!i||n.pointerId!==i.pointerId)return;let{room:h,moving:m}=i;if(a(),!m)return;let v=_=>{_.stopPropagation(),_.preventDefault()};s.addEventListener("click",v,!0),setTimeout(()=>s.removeEventListener("click",v,!0),0),r(h,qe(s,n.clientX,n.clientY))}function l(n){if(!i||n&&n.pointerId!==void 0&&n.pointerId!==i.pointerId)return;let{room:h,moving:m}=i;a(),m&&r(h,null)}function p(n){n.key==="Escape"&&l()}function f(n){if(i||n.isPrimary===!1||n.button>0)return;let h=n.composedPath().find(m=>m.dataset?.dragRoom);if(h){i={handle:h,room:h.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{h.setPointerCapture?.(n.pointerId)}catch{}h.addEventListener("pointermove",o),h.addEventListener("pointerup",u),h.addEventListener("pointercancel",l),h.addEventListener("lostpointercapture",l),s.addEventListener("keydown",p,!0)}}return{begin:f,cancel:()=>l(),active:()=>!!i?.moving}}var pt=[],Ge=s=>String(s).split("/").filter(Boolean);function E(s){let{id:t,path:e,title:r,render:i}=s??{};if(typeof t!="string"||!t||t==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof i!="function")throw new Error(`the screen '${t}' has a title and a render`);let a=Ge(e);if(a.length===0)throw new Error(`the screen '${t}' has a path`);let o=u=>u.map(l=>l.startsWith(":")?":":l).join("/");for(let u of pt){if(u.id===t)throw new Error(`the screen '${t}' is registered twice`);if(o(u.segments)===o(a))throw new Error(`the screens '${u.id}' and '${t}' have the same path`)}pt.push({id:t,segments:a,title:r,render:i})}function jt(s){return pt.find(t=>t.id===s)??null}var ft="#/",mt=Object.freeze({screen:"home",params:Object.freeze({}),address:ft});function z(s,t={}){let e=jt(s);if(!e)throw new Error(`there is no screen '${s}'`);return`#/${e.segments.map(i=>{if(!i.startsWith(":"))return i;let a=t[i.slice(1)];if(typeof a!="string"||!a)throw new Error(`the screen '${s}' needs '${i.slice(1)}'`);return encodeURIComponent(a)}).join("/")}`}function Ve(s){let t;try{t=Ge(String(s??"").replace(/^#/,"")).map(e=>decodeURIComponent(e))}catch{return mt}for(let e of pt){if(e.segments.length!==t.length)continue;let r={};if(e.segments.every((a,o)=>a.startsWith(":")?(r[a.slice(1)]=t[o],!0):a===t[o]))return{screen:e.id,params:r,address:z(e.id,r)}}return mt}function Ke(s=globalThis){let t=new Set,e=()=>Ve(s.location?.hash??""),r=()=>{let i=e();for(let a of[...t])a(i)};return{route:e,open(i){let a=Ve(i);a.address!==e().address&&(s.history.pushState({chorus:!0},"",a.address),r())},back(){if(e().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",ft),r()}},watch(i){let a=o=>i(o);return t.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),t.add(a),a(e()),()=>{t.delete(a),t.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var Zr=(s,t)=>Ct("../",s,t),D=s=>typeof s=="string"&&s?s:null,ts=["playing","paused","buffering"];function Ye(s,t=Zr){let e=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let i of e){if(!i||typeof i!="object"||typeof i.id!="string"||!i.id)continue;let a=i.now_playing&&typeof i.now_playing=="object"?i.now_playing:null,o=a?D(a.art_url):null;r.set(i.id,{source:D(i.source),nowPlaying:a&&{title:D(a.title),artist:D(a.artist),album:D(a.album),state:ts.includes(a.state)?a.state:null,via:D(a.via),artwork:o?t(i.id,o):null}})}return r}var Bt={source:null,nowPlaying:null};function es(s){let t=s&&Array.isArray(s.inputs)?s.inputs:[],e=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return t.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:e.get(r)??r}))}function rs(s){let t=s&&s.sound&&typeof s.sound=="object"?s.sound:{},e=i=>Number.isInteger(i)?i:null,r=i=>typeof i=="boolean"?i:null;return{bass:e(t.bass),treble:e(t.treble),loudness:r(t.loudness),night:r(t.night),speech:r(t.speech)}}function ss(s){let t=s&&typeof s=="object"?s:{},e=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:L(t.limit),effectiveLimit:L(t.effective_limit),quietEnabled:typeof t.quiet_enabled=="boolean"?t.quiet_enabled:null,windows:(Array.isArray(t.quiet)?t.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(i=>typeof i=="string"),start:e(r.start),end:e(r.end),limit:L(r.limit),active:r.active===!0}))}}function Xe(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(e=>e&&typeof e.input=="string"&&e.input&&typeof e.target=="string").map(e=>({input:e.input,target:e.target,enabled:e.enabled===!0,stopOnStandby:e.stop_on_standby!==!1,lowLatency:e.low_latency!==!1}))}function Qe(s){let t=s&&Array.isArray(s.alarms)?s.alarms:[],e=r=>Number.isInteger(r)&&r>=0?r:0;return t.filter(r=>r&&typeof r.alarm=="string"&&r.alarm&&typeof r.target=="string").map(r=>({id:r.alarm,target:r.target,time:typeof r.time=="string"?r.time:"",days:(Array.isArray(r.days)?r.days:[]).filter(i=>typeof i=="string"),source:typeof r.source=="string"?r.source:"",volume:L(r.volume)??0,rampS:e(r.ramp_s),durationMin:e(r.duration_min),enabled:r.enabled===!0,ringing:r.ringing===!0}))}function Ze(s){return(s&&Array.isArray(s.sleep)?s.sleep:[]).filter(e=>e&&typeof e.target=="string"&&e.target).map(e=>({target:e.target,minutes:Number.isInteger(e.minutes)?e.minutes:null,remainingS:Number.isInteger(e.remaining_s)&&e.remaining_s>=0?e.remaining_s:null}))}function tr(s){return(s&&Array.isArray(s.stored_sources)?s.stored_sources:[]).filter(e=>e&&typeof e.id=="string"&&e.id&&typeof e.kind=="string").map(e=>({id:e.id,kind:e.kind,value:typeof e.value=="string"?e.value:"",name:typeof e.name=="string"&&e.name?e.name:e.id}))}function er(s){return!s||!Array.isArray(s.chimes)?null:s.chimes.filter(t=>typeof t=="string"&&t)}function rr(s){let t=s&&s.soloist&&typeof s.soloist=="object"?s.soloist:null;return t?(Array.isArray(t.receivers)?t.receivers:[]).filter(e=>e&&e.state==="running"&&typeof e.target=="string"&&e.target).map(e=>e.target):null}function U(s,t){return(Array.isArray(s)?s:[]).find(e=>e.id===t)??null}function is(s,t,e){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let r=Array.isArray(s.bond)?s.bond:[],i=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:L(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:rs(s),limits:ss(s),group:i,...i===s.id&&e.get(i)||Bt,bond:r.filter(a=>a&&typeof a.endpoint=="string"&&typeof a.role=="string").map(a=>({endpoint:a.endpoint,name:t.get(a.endpoint)??a.endpoint,role:a.role}))}}function sr(s,t){let e=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],i=new Map(r.filter(o=>o&&typeof o.id=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.id,o.name])),a=Ye(s,t);return e.map(o=>is(o,i,a)).filter(Boolean)}function L(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function as(s,t){let e=new Map(sr(s,t).map(n=>[n.id,n.name])),r=Ye(s,t),i=n=>({id:n,name:e.get(n)??n}),a=n=>Array.isArray(n)?n:[],o=n=>a(n).filter(h=>typeof h=="string"&&h).map(i),u=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=a(s?.groups).filter(u),p=a(s?.saved_groups).filter(u),f=new Set(p.map(n=>n.id));return[...p.map(n=>{let h=l.find(m=>m.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:o(n.zones),rooms:h?o(h.zones):[],volume:h?L(h.volume):null,...h&&r.get(n.id)||Bt}}),...l.filter(n=>n.kind==="live"&&!f.has(n.id)).map(n=>{let h=o(n.zones);return{id:n.id,name:h.map(m=>m.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:h,volume:L(n.volume),...r.get(n.id)??Bt}})]}var Ht=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function ir(s){let t=new Set,e=null,r=[],i=[],a=[],o="connecting",u=!1,l=null,p=()=>({state:e,rooms:r,groups:i,inputs:a,status:o}),f=()=>{let g=p();for(let y of[...t])y(g)},n=g=>{e=g,r=sr(g,s.artwork),i=as(g,s.artwork),a=es(g)};function h(){l||(l=s.events({onState(g){Ht(g)&&(u=!0,n(g),f())},onStatus(g){o!==g&&(o=g,f())}}),s.state().then(g=>{u||!Ht(g)||(n(g),f())},()=>{}))}function m(){l?.(),l=null}async function v(g){let y=await s.command(g);return y.signedOut&&o!=="signed-out"&&(o="signed-out",f()),y.ok&&Ht(y.state)&&(!e||y.state.serial>e.serial)&&(n(y.state),f()),y}function _(g){return t.add(g),g(p()),()=>t.delete(g)}return{start:h,stop:m,command:v,subscribe:_,view:p}}var gt=s=>`alarm:${s}`,ar="alarms:draft",or=s=>`stored:${s}`,nr="stored:draft:",lr=s=>`sleep:${s}`,dr="sleep:draft:",Wt={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},vt={url:"Stream URL",spotify:"Spotify URI"},os=Object.freeze({alarm:"",target:"",time:"07:00",days:Object.freeze(["mon","tue","wed","thu","fri"]),source:"",volume:300,rampS:30,durationMin:60,enabled:!0}),ns=Object.freeze({id:"",name:"",kind:"url",value:""}),ls=Object.freeze({target:"",minutes:30}),qt=s=>`${Math.round(s/10)}%`,ds=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),ur=(s,t)=>Math.min(t,Math.max(0,Math.round(Number(s)||0)));function us(s){let t=Math.max(0,Math.floor(s)),e=Math.floor(t/3600),r=Math.floor(t%3600/60);return e>0?`${e} h ${r} min left`:r>0?`${r} min ${t%60} s left`:`${t} s left`}var Vt=class extends b{static properties={known:{type:Boolean},heard:{attribute:!1},alarms:{attribute:!1},stored:{attribute:!1},sleep:{attribute:!1},chimes:{attribute:!1},receivers:{attribute:!1},inputs:{attribute:!1},rooms:{attribute:!1},savedGroups:{attribute:!1},formedGroups:{attribute:!1},refusals:{attribute:!1},refusalFields:{attribute:!1},_alarm:{state:!0},_source:{state:!0},_timer:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.known=!1,this.heard=null,this.alarms=[],this.stored=[],this.sleep=[],this.chimes=null,this.receivers=null,this.inputs=[],this.rooms=[],this.savedGroups=[],this.formedGroups=[],this.refusals={},this.refusalFields={},this._alarm={...os},this._source={...ns},this._timer={...ls},this.clock=()=>globalThis.performance.now(),this._heardAt=0,this._ticker=null}disconnectedCallback(){super.disconnectedCallback(),this._tickEvery(!1)}willUpdate(t){t.has("heard")&&(this._heardAt=this.clock())}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-holds]")){let e=t.dataset.holds;t.value!==e&&(t.value=e)}this._tickEvery(this.isConnected&&(this.sleep??[]).some(t=>t.remainingS!==null))}_tickEvery(t){t!==(this._ticker!==null)&&(t?this._ticker=globalThis.setInterval(()=>this.tick(),1e3):(globalThis.clearInterval(this._ticker),this._ticker=null))}tick(){this.requestUpdate()}_left(t){let e=Math.floor(Math.max(0,this.clock()-this._heardAt)/1e3);return Math.max(0,t.remainingS-e)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:t,body:e},bubbles:!0,composed:!0}))}_refusal(t){let e=this.refusals?.[t]??"";if(!e)return d`<p role="alert"></p>`;let r=this.refusalFields?.[t]??"";return d`<p role="alert" data-refusal-field=${r||c}>Refused${r?` (${r})`:""}: ${e}</p>`}_place(t){return[...this.rooms??[],...this.savedGroups??[],...this.formedGroups??[]].find(r=>r.id===t)?.name??t}_storedOf(t){return t.startsWith("stored:")?(this.stored??[]).find(e=>e.id===t.slice(7))??null:null}_sourceName(t){if(t.startsWith("chime:"))return`Chime: ${t.slice(6)}`;if(t.startsWith("line-in:"))return`Input: ${(this.inputs??[]).find(i=>i.source===t)?.label??t.slice(8)}`;let e=this._storedOf(t);return e?`${vt[e.kind]??e.kind}: ${e.name}`:t}_unplayable(t,e){if(t.startsWith("chime:"))return this.chimes!==null&&!this.chimes.includes(t.slice(6))?`This server has no chime "${t.slice(6)}".`:"";if(t.startsWith("line-in:"))return(this.inputs??[]).some(r=>r.source===t)?"":`The input ${t.slice(8)} is not offered now: its speaker is not connected.`;if(t.startsWith("stored:")){let r=this._storedOf(t);if(!r)return`This server has no stored source "${t.slice(7)}".`;if(r.kind!=="spotify")return"";if(this.receivers===null)return"This server runs no Spotify receiver.";let i=(this.savedGroups??[]).some(a=>a.id===e);return this.receivers.includes(`${i?"group":"room"}:${e}`)?"":`No Spotify receiver is running for ${this._place(e)}.`}return"This is not a source an alarm plays."}_alarmOf(t){return(this.alarms??[]).find(e=>e.id===t)??null}_sendable(t,e={}){return Ot({...t,alarm:t.id,...e})}_onSwitch(t){let e=this._alarmOf(t.currentTarget.dataset.alarm);e&&this._ask(gt(e.id),this._sendable(e,{enabled:!e.enabled}))}_onStop(t){let e=t.currentTarget.dataset.alarm;this._ask(gt(e),we(e))}_onDelete(t){let e=t.currentTarget.dataset.alarm;this._ask(gt(e),ye(e))}_onEdit(t){let e=this._alarmOf(t.currentTarget.dataset.alarm);if(!e)return;let{id:r,ringing:i,...a}=e;this._alarm={alarm:r,...a}}_alarmRow(t){let e=t.days.length===0?"once":w.filter(a=>t.days.includes(a)).map(a=>Wt[a][0]).join(" "),r=t.durationMin===0?"until stopped":`for ${t.durationMin} min`,i=this._unplayable(t.source,t.target);return d`
      <li data-alarm=${t.id} ?data-ringing=${t.ringing}>
        <h4>${t.id}</h4>
        <p data-value="when">${t.time}, ${e}</p>
        <p data-value="what">
          ${this._sourceName(t.source)} in ${this._place(t.target)}, to ${qt(t.volume)} over ${t.rampS} s,
          ${r}
        </p>
        ${i?d`<p data-fallback>${i} The alarm rings the bell chime instead.</p>`:c}
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
          ${t.ringing?d`<span data-value="ringing">Ringing now</span>
                <button type="button" data-alarm=${t.id} aria-label="Stop alarm ${t.id}" @click=${this._onStop}>
                  Stop
                </button>`:c}
          <button type="button" data-alarm=${t.id} aria-label="Edit alarm ${t.id}" @click=${this._onEdit}>Edit</button>
          <button type="button" data-alarm=${t.id} aria-label="Delete alarm ${t.id}" @click=${this._onDelete}>
            Delete
          </button>
        </div>
        ${this._refusal(gt(t.id))}
      </li>
    `}_offeredSources(){let t=e=>(this.stored??[]).filter(r=>r.kind===e).map(r=>({value:`stored:${r.id}`,name:r.name}));return[{kind:"chime",label:"Chimes",options:(this.chimes??[]).map(e=>({value:`chime:${e}`,name:e}))},{kind:"line-in",label:"Inputs",options:(this.inputs??[]).map(e=>({value:e.source,name:e.label}))},{kind:"url",label:"Stored stream URLs",options:t("url")},{kind:"spotify",label:"Stored Spotify URIs",options:t("spotify")}]}_alarmDraft(){let t=this._alarm,e=[...this.rooms??[],...this.savedGroups??[]],r=this._offeredSources().flatMap(i=>i.options)[0];return{...t,target:t.target||(e[0]?.id??""),source:t.source||(r?.value??"")}}_setAlarm(t){this._alarm={...this._alarm,...t}}_onAlarmText(t){this._setAlarm({alarm:t.target.value.trim()})}_onAlarmChoice(t){this._setAlarm({[t.target.dataset.field]:t.target.value})}_onAlarmTime(t){if(!ds(t.target.value)){t.target.value=this._alarm.time;return}this._setAlarm({time:t.target.value})}_onAlarmDay(t){let e=t.currentTarget.dataset.day,r=this._alarm.days.includes(e)?this._alarm.days.filter(i=>i!==e):w.filter(i=>i===e||this._alarm.days.includes(i));this._setAlarm({days:r})}_onAlarmVolume(t){this._setAlarm({volume:Number(t.target.value)})}_onAlarmCount(t){let{field:e,max:r}=t.target.dataset,i=ur(t.target.value,Number(r));t.target.value=String(i),this._setAlarm({[e]:i})}_onAlarmEnabled(){this._setAlarm({enabled:!this._alarm.enabled})}_onSave(){this._ask(ar,Ot(this._alarmDraft()))}_kindNotes(t){let e=[];this.chimes===null&&e.push(["chime","This server does not say which chimes it has, so none is offered here."]),(this.inputs??[]).length===0&&e.push(["line-in","No input is offered now: no speaker with a line-in is connected."]);let r=new Set((this.stored??[]).map(o=>o.kind));r.has("url")||e.push(["url","No stream URL is stored: add one under Stored sources."]),r.has("spotify")?this.receivers===null&&e.push(["spotify","This server runs no Spotify receiver: an alarm with a Spotify URI rings the bell chime instead."]):e.push(["spotify","No Spotify URI is stored: add one under Stored sources."]);let i=t.source?this._unplayable(t.source,t.target):"",a=this._storedOf(t.source)?.kind==="spotify"?"spotify":"chosen";return i&&!(a==="spotify"&&this.receivers===null)&&e.push([a,`${i} The alarm would ring the bell chime instead.`]),e.map(([o,u])=>d`<p data-unavailable=${o}>${u}</p>`)}_alarmForm(){let t=this._alarmDraft(),e=this.rooms??[],r=this.savedGroups??[],i=[...e,...r],a=this._offeredSources(),o=n=>d`<option value=${n.value}>${n.name}</option>`,u=n=>d`<option value=${n.id}>${n.name}</option>`,l=a.some(n=>n.options.some(h=>h.value===t.source)),p=this._alarmOf(t.alarm)!==null,f=t.alarm!==""&&t.target!==""&&t.source!=="";return d`
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
            ${i.some(n=>n.id===t.target)||!t.target?c:d`<option value=${t.target}>${t.target} (not on this server now)</option>`}
            ${e.length===0?c:d`<optgroup label="Rooms">${e.map(u)}</optgroup>`}
            ${r.length===0?c:d`<optgroup label="Saved groups">${r.map(u)}</optgroup>`}
          </select>
          <label for="alarm-time">At</label>
          <input id="alarm-time" type="time" .value=${t.time} aria-label="Alarm time" @change=${this._onAlarmTime} />
        </div>
        <div class="row" role="group" aria-label="Days of the alarm">
          ${w.map(n=>d`<button
                type="button"
                data-day=${n}
                aria-label="${Wt[n][1]}, the alarm"
                aria-pressed=${t.days.includes(n)?"true":"false"}
                @click=${this._onAlarmDay}
              >
                ${Wt[n][0]}
              </button>`)}
          <p data-value="days">${t.days.length===0?"No day: it rings once, at the next such time.":"It rings on these days."}</p>
        </div>
        <div class="row">
          <label for="alarm-source">Plays</label>
          <select id="alarm-source" data-field="source" data-holds=${t.source} aria-label="Alarm source" @change=${this._onAlarmChoice}>
            ${l||!t.source?c:d`<option value=${t.source}>${t.source} (not on this server now)</option>`}
            ${a.map(n=>n.options.length===0?c:d`<optgroup label=${n.label} data-kind=${n.kind}>${n.options.map(o)}</optgroup>`)}
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
            aria-valuetext=${qt(t.volume)}
            @input=${this._onAlarmVolume}
          />
          <span class="figure" data-value="volume">${qt(t.volume)}</span>
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
        ${this._refusal(ar)}
      </div>
    `}_onForget(t){let e=t.currentTarget.dataset.stored;this._ask(or(e),Se(e))}_onSourceField(t){this._source={...this._source,[t.target.dataset.field]:t.target.value.trim()}}_onStore(){let{id:t,kind:e,value:r,name:i}=this._source;this._ask(nr,ke(t,e,r,i||t))}_storedRow(t){return d`
      <li data-stored=${t.id}>
        <h4>${t.name}</h4>
        <p><span data-value="kind">${vt[t.kind]??t.kind}</span>, <span data-id>${t.id}</span></p>
        <p data-value="value">${t.value}</p>
        <div class="row">
          <button type="button" data-stored=${t.id} aria-label="Forget stored source ${t.name}" @click=${this._onForget}>
            Forget
          </button>
        </div>
        ${this._refusal(or(t.id))}
      </li>
    `}_storedForm(){let t=this._source,e=t.kind==="spotify";return d`
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
        ${this._refusal(nr)}
      </div>
    `}_sleepTargets(){return[...this.rooms??[],...this.formedGroups??[]]}_onCancel(t){let e=t.currentTarget.dataset.target;this._ask(lr(e),Tt(e,0))}_onSleepTarget(t){this._timer={...this._timer,target:t.target.value}}_onSleepMinutes(t){let e=ur(t.target.value,st);t.target.value=String(e),this._timer={...this._timer,minutes:e}}_onSleep(){let t=this._timer.target||(this._sleepTargets()[0]?.id??"");t&&this._ask(dr,Tt(t,this._timer.minutes))}_sleepRow(t){let e=this._place(t.target),r=t.remainingS===null?`${t.minutes??"?"} min asked for`:us(this._left(t));return d`
      <li data-sleep=${t.target}>
        <h4>${e}</h4>
        <div class="row">
          <span class="figure" data-value="left">${r}</span>
          <button type="button" data-target=${t.target} aria-label="Cancel sleep timer for ${e}" @click=${this._onCancel}>
            Cancel
          </button>
        </div>
        ${this._refusal(lr(t.target))}
      </li>
    `}_sleepForm(){let t=this.rooms??[],e=this.formedGroups??[],r=this._timer.target||(this._sleepTargets()[0]?.id??""),i=a=>d`<option value=${a.id}>${a.name}</option>`;return d`
      <div class="draft" data-draft="sleep">
        <div class="row">
          <label for="sleep-target">For</label>
          <select id="sleep-target" data-holds=${r} aria-label="Sleep timer target" @change=${this._onSleepTarget}>
            ${t.length===0?c:d`<optgroup label="Rooms">${t.map(i)}</optgroup>`}
            ${e.length===0?c:d`<optgroup label="Groups playing now">${e.map(i)}</optgroup>`}
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
        ${this._refusal(dr)}
      </div>
    `}render(){if(!this.known)return d`<p role="status" data-missing>Reading this server's alarms.</p>`;let t=this.alarms??[],e=this.stored??[],r=this.sleep??[];return d`
      <h2>Alarms</h2>
      <p>An alarm rings in its room or its saved group on the server's own clock, and rises from silence to its volume.</p>
      ${t.length===0?d`<p role="status" data-none="alarms">This server has no alarm.</p>`:d`<ul aria-label="Alarms">
            ${t.map(i=>this._alarmRow(i))}
          </ul>`}
      <h3>Set an alarm</h3>
      ${this._alarmForm()}

      <h2>Stored sources</h2>
      <p>A stream URL or a Spotify URI the server keeps for an alarm to play.</p>
      ${e.length===0?d`<p role="status" data-none="stored">This server has no stored source.</p>`:d`<ul aria-label="Stored sources">
            ${e.map(i=>this._storedRow(i))}
          </ul>`}
      <h3>Store a source</h3>
      ${this._storedForm()}

      <h2>Sleep timers</h2>
      ${r.length===0?d`<p role="status" data-none="sleep">No sleep timer is running.</p>`:d`<ul aria-label="Sleep timers">
            ${r.map(i=>this._sleepRow(i))}
          </ul>`}
      <h3>Start a sleep timer</h3>
      ${this._sleepForm()}
    `}};customElements.define("chorus-alarms",Vt);var Gt="alarms",Jt=s=>s.map(({id:t,name:e})=>({id:t,name:e}));E({id:Gt,path:"alarms",title:()=>"Alarms and sleep timers",render:(s,{view:t,refusals:e,refusalFields:r})=>{let i=t.groups??[];return d`
      <chorus-alarms
        .known=${t.state!==null}
        .heard=${t.state}
        .alarms=${Qe(t.state)}
        .stored=${tr(t.state)}
        .sleep=${Ze(t.state)}
        .chimes=${er(t.state)}
        .receivers=${rr(t.state)}
        .inputs=${t.inputs??[]}
        .rooms=${Jt(t.rooms)}
        .savedGroups=${Jt(i.filter(a=>a.kind==="saved"))}
        .formedGroups=${Jt(i.filter(a=>a.rooms.length>0))}
        .refusals=${e}
        .refusalFields=${r}
      ></chorus-alarms>
    `}});function Z(s,t){return t.find(e=>e.id===s.group&&e.rooms.some(r=>r.id===s.id))??null}function cr(s,t,e){if(!s||!t)return null;let r=Z(s,e);return t.kind==="alone"?r?tt(s.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?r&&r.id===t.id?null:xt(s.id,t.id):t.kind==="room"?t.id===s.id||r&&r.rooms.some(i=>i.id===t.id)?null:xt(s.id,t.id):null}var Kt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function hr(s){if(s==="alone")return{kind:"alone"};let t=String(s).indexOf(":");if(t<1)return null;let e=s.slice(0,t),r=s.slice(t+1);return(e==="room"||e==="group")&&r?{kind:e,id:r}:null}function pr(s,t){let e=Z(s,t);return e?Kt({kind:"group",id:e.id}):"alone"}function mr(s,t,e){return[{value:"alone",label:"Alone"},...e.map(r=>({value:Kt({kind:"group",id:r.id}),label:r.name})),...t.filter(r=>r.id!==s.id&&!Z(r,e)).map(r=>({value:Kt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var fr=s=>`autoplay:${s}`;function cs(s){let t=Xe(s.state),e=o=>t.find(u=>u.input===o)??null,r=(s.inputs??[]).map(o=>({input:o.id,label:o.label,offered:!0,rule:e(o.id)})),i=new Set(r.map(o=>o.input)),a=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(o=>o&&typeof o.input=="string"&&typeof o.name=="string"&&o.name).map(o=>[o.input,o.name]));return[...r,...t.filter(o=>!i.has(o.input)).map(o=>({input:o.input,label:a.get(o.input)??o.input,offered:!1,rule:o}))]}var Yt=class extends b{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={}}_row(t){return(this.rows??[]).find(e=>e.input===t)??null}_ask(t,e,r){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:fr(t.input),body:_e(t.input,e,r,t.rule??{})},bubbles:!0,composed:!0}))}_onSwitch(t){let e=this._row(t.currentTarget.dataset.input);e?.rule&&this._ask(e,e.rule.target,!e.rule.enabled)}_onTarget(t){let e=this._row(t.target.dataset.input),r=t.target.value,i=e?.rule?.target??"";t.target.value=i,!(!e||!r||r===i)&&this._ask(e,r,e.rule?.enabled??!1)}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-input]")){let e=this._row(t.dataset.input)?.rule?.target??"";t.value!==e&&(t.value=e)}}_targets(t){let e=this.rooms??[],r=this.groups??[],i=!t||[...e,...r].some(o=>o.id===t.target),a=o=>d`<option value=${o.id} ?selected=${t?.target===o.id}>${o.name}</option>`;return d`
      ${t?c:d`<option value="" selected>Nowhere yet</option>`}
      ${i?c:d`<option value=${t.target} selected>${t.target} (not on this server now)</option>`}
      ${e.length===0?c:d`<optgroup label="Rooms">${e.map(a)}</optgroup>`}
      ${r.length===0?c:d`<optgroup label="Saved groups">${r.map(a)}</optgroup>`}
    `}_input(t){let{input:e,label:r,offered:i,rule:a}=t,o=this.refusals?.[fr(e)]??"",u=a?a.enabled?"On":"Off":"Choose where it plays, then switch it on";return d`
      <li data-input=${e}>
        <h3>${r}</h3>
        ${r===e?c:d`<p data-id>${e}</p>`}
        ${i?c:d`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${e}
            aria-label="Autoplay for ${r}"
            aria-pressed=${a?.enabled?"true":"false"}
            ?disabled=${!a}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${u}</span>
        </div>
        <div class="row">
          <label for="target-${e}">Plays in</label>
          <select id="target-${e}" data-input=${e} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(a)}
          </select>
        </div>
        <p role="alert">${o?`Refused: ${o}`:c}</p>
      </li>
    `}render(){return this.rows===null?d`<p role="status" data-missing>Reading this server's inputs.</p>`:d`
      <h2>Autoplay</h2>
      <p>An input with a rule that is on plays in its room or its group when its signal arrives.</p>
      ${this.rows.length===0?d`<p role="status" data-none>This server offers no input now, and has no autoplay rule.</p>`:d`<ul aria-label="Inputs">
            ${this.rows.map(t=>this._input(t))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",Yt);var Xt="autoplay",gr=s=>s.map(({id:t,name:e})=>({id:t,name:e}));E({id:Xt,path:"autoplay",title:()=>"Autoplay",render:(s,{view:t,refusals:e})=>d`
    <chorus-autoplay
      .rows=${t.state===null?null:cs(t)}
      .rooms=${gr(t.rooms)}
      .groups=${gr((t.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${e}
    ></chorus-autoplay>
  `});var vr={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},bt=s=>(...t)=>({_$litDirective$:s,values:t}),F=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,r){this._$Ct=t,this._$AM=e,this._$Ci=r}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:hs}=Be,br=s=>s;var $r=()=>document.createComment(""),j=(s,t,e)=>{let r=s._$AA.parentNode,i=t===void 0?s._$AB:t._$AA;if(e===void 0){let a=r.insertBefore($r(),i),o=r.insertBefore($r(),i);e=new hs(a,o,s,s.options)}else{let a=e._$AB.nextSibling,o=e._$AM,u=o!==s;if(u){let l;e._$AQ?.(s),e._$AM=s,e._$AP!==void 0&&(l=s._$AU)!==o._$AU&&e._$AP(l)}if(a!==i||u){let l=e._$AA;for(;l!==a;){let p=br(l).nextSibling;br(r).insertBefore(l,i),l=p}}}return e},O=(s,t,e=s)=>(s._$AI(t,e),s),ps={},$t=(s,t=ps)=>s._$AH=t,_r=s=>s._$AH,_t=s=>{s._$AR(),s._$AA.remove()};var yr=(s,t,e)=>{let r=new Map;for(let i=t;i<=e;i++)r.set(s[i],i);return r},yt=bt(class extends F{constructor(s){if(super(s),s.type!==vr.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,t,e){let r;e===void 0?e=t:t!==void 0&&(r=t);let i=[],a=[],o=0;for(let u of s)i[o]=r?r(u,o):o,a[o]=e(u,o),o++;return{values:a,keys:i}}render(s,t,e){return this.dt(s,t,e).values}update(s,[t,e,r]){let i=_r(s),{values:a,keys:o}=this.dt(t,e,r);if(!Array.isArray(i))return this.ut=o,a;let u=this.ut??=[],l=[],p,f,n=0,h=i.length-1,m=0,v=a.length-1;for(;n<=h&&m<=v;)if(i[n]===null)n++;else if(i[h]===null)h--;else if(u[n]===o[m])l[m]=O(i[n],a[m]),n++,m++;else if(u[h]===o[v])l[v]=O(i[h],a[v]),h--,v--;else if(u[n]===o[v])l[v]=O(i[n],a[v]),j(s,l[v+1],i[n]),n++,v--;else if(u[h]===o[m])l[m]=O(i[h],a[m]),j(s,i[n],i[h]),h--,m++;else if(p===void 0&&(p=yr(o,m,v),f=yr(u,n,h)),p.has(u[n]))if(p.has(u[h])){let _=f.get(o[m]),g=_!==void 0?i[_]:null;if(g===null){let y=j(s,i[n]);O(y,a[m]),l[m]=y}else l[m]=O(g,a[m]),j(s,i[n],g),i[_]=null;m++}else _t(i[h]),h--;else _t(i[n]),n++;for(;m<=v;){let _=j(s,l[v+1]);O(_,a[m]),l[m++]=_}for(;n<=h;){let _=i[n++];_!==null&&_t(_)}return this.ut=o,$t(s,l),A}});var wr=bt(class extends F{constructor(){super(...arguments),this.key=c}render(s,t){return this.key=s,t}update(s,[t,e]){return t!==this.key&&($t(s),this.key=t),e}});var ms={playing:"Playing",paused:"Paused",buffering:"Buffering"};function fs(s,t=[]){if(!s)return"Unavailable";let e=t.find(o=>o.source===s);if(e)return e.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...i]=s.split(":"),a=i.join(":");return r==="line-in"&&a?`Input ${a}`:r==="player"&&a?`Network player ${a}`:r==="chime"&&a?`Chime ${a}`:r==="soloist"&&a?"Spotify":s}var Qt=class extends b{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:me(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=d`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:wr(t.artwork,d`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,e=this.inputs??[];return d`
      ${t?d`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?d`<p data-artist>${t.artist}</p>`:c}
              ${t.album?d`<p data-album>${t.album}</p>`:c}
              <p data-state>${ms[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${fs(this.source,e)}</p>
      ${this.pick&&e.length>0?d`<ul aria-label="Inputs for ${this.name}">
            ${e.map(r=>d`<li data-input=${r.id}>
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
    `}};customElements.define("chorus-playing",Qt);var gs=s=>`${Math.round(s/10)}%`,Zt=class extends b{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let r=t.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(fe(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(tt(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(a=>a.id)),r=t.defined??[],i=new Set(r.map(a=>a.id));return[...r.map(a=>({...a,playing:e.has(a.id)})),...t.rooms.filter(a=>!i.has(a.id)).map(a=>({...a,playing:!0}))]}render(){let t=this.group;if(!t)return c;let e=t.volume===null?"":gs(this._dragged??t.volume);return d`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?c:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(r=>d`<li data-member=${r.id} data-playing=${String(r.playing)}>
              <span>${r.name}</span>
              ${r.playing?d`<button
                    type="button"
                    aria-label="Remove ${r.name} from ${t.name}"
                    @click=${()=>this._onRemove(r)}
                  >
                    Remove
                  </button>`:d`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.source?d`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${t.kind==="live"||t.active===!0}
          ></chorus-playing>`:c}
      ${t.kind==="saved"&&!t.active?d`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:c}
      ${t.volume===null?c:d`<div class="row">
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
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-group-card",Zt);var te=class extends b{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return d`
      ${this.groups!==null&&t.length===0?d`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${yt(t,r=>r.id,r=>d`<li
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
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:c}
      </p>
    `}};customElements.define("chorus-groups",te);var kr=Object.freeze(["phone","desktop"]),vs=48,bs=`(min-width: ${vs}em)`;function Sr(s,t=globalThis){if(typeof t?.matchMedia!="function")return s("phone"),()=>{};let e=t.matchMedia(bs),r=()=>s(e.matches?"desktop":"phone");return e.addEventListener("change",r),r(),()=>e.removeEventListener("change",r)}var ee=s=>`limits:${s}`,Ar={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},$s=Object.freeze({days:w,start:"22:00",end:"07:00",limit:250}),P=s=>`${Math.round(s/10)}%`,xr=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),_s=({days:s,start:t,end:e,limit:r})=>({days:s,start:t,end:e,limit:r}),re=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...$s},this._held=new Set,this._asked=null,this._unanswered=0}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||e)&&(r.value=r.dataset.server)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:ee(this.room.id),body:t,done:e},bubbles:!0,composed:!0}))}_askWindows(t){let e=this.room.id,r=(this._asked?.room===e?this._asked.windows:this.room.limits.windows).map(_s);t(r),this._asked={room:e,windows:r},this._unanswered+=1,this._ask(be(e,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onFocus(t){this._held.add(t.target.dataset.key)}_onBlur(t){let{key:e,server:r}=t.target.dataset;this._held.delete(e),this._release(e),t.target.value=r}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.key]:Number(t.target.value)}}_onLimitChange(t){this._release(t.target.dataset.key),this._ask(ve(this.room.id,Number(t.target.value)))}_onEnabled(){this._ask($e(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(t){let e=Number(t.target.dataset.window),r=Number(t.target.value);this._release(t.target.dataset.key),this._askWindows(i=>{i[e]={...i[e],limit:r}})}_onWindowTime(t){let{window:e,edge:r,server:i}=t.target.dataset,a=t.target.value;if(!xr(a)){t.target.value=i;return}a!==i&&this._askWindows(o=>{o[Number(e)]={...o[Number(e)],[r]:a}})}_onWindowDay(t){let{window:e,day:r}=t.currentTarget.dataset;this._askWindows(i=>{let a=i[Number(e)],o=a.days.includes(r)?a.days.filter(u=>u!==r):[...a.days,r];i[Number(e)]={...a,days:o}})}_onRemove(t){let e=Number(t.currentTarget.dataset.window);this._askWindows(r=>r.splice(e,1))}_onDraftDay(t){let e=t.currentTarget.dataset.day,r=this._draft.days.includes(e)?this._draft.days.filter(i=>i!==e):w.filter(i=>i===e||this._draft.days.includes(i));this._draft={...this._draft,days:r}}_onDraftTime(t){let e=t.target.dataset.edge;if(!xr(t.target.value)){t.target.value=this._draft[e];return}this._draft={...this._draft,[e]:t.target.value}}_onDraftLimit(t){this._draft={...this._draft,limit:Number(t.target.value)}}_onAdd(){this._askWindows(t=>t.push({...this._draft}))}_days(t,e,r,i){let a=this.room;return d`
      <div class="row" role="group" aria-label="Days of ${e} for ${a.name}">
        ${w.map(o=>d`<button
              type="button"
              data-day=${o}
              data-window=${i??c}
              aria-label="${Ar[o][1]}, ${e} for ${a.name}"
              aria-pressed=${t.includes(o)?"true":"false"}
              @click=${r}
            >
              ${Ar[o][0]}
            </button>`)}
      </div>
    `}_window(t,e,r){let i=this.room,a=`window ${e+1}`,o=i.limits.quietEnabled!==!1,u=t.active?o?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return d`<li data-window=${e}><p data-value="active">Unavailable</p></li>`;let l=`window-${e}`;return d`
      <li data-window=${e} ?data-active=${t.active}>
        <div class="row">
          <strong>Window ${e+1}</strong>
          <span data-value="active" ?data-active=${t.active&&o}>${u}</span>
        </div>
        ${this._days(t.days,a,this._onWindowDay,e)}
        <div class="row">
          <label for="${l}-start">From</label>
          <input
            id="${l}-start"
            type="time"
            data-key="${l}-start"
            data-window=${e}
            data-edge="start"
            data-server=${t.start}
            aria-label="Start of ${a} for ${i.name}"
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @change=${this._onWindowTime}
          />
          <label for="${l}-end">Until</label>
          <input
            id="${l}-end"
            type="time"
            data-key="${l}-end"
            data-window=${e}
            data-edge="end"
            data-server=${t.end}
            aria-label="End of ${a} for ${i.name}"
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
            data-window=${e}
            data-server=${t.limit}
            aria-label="Limit of ${a} for ${i.name}"
            aria-valuetext=${P(this._dragged[`${l}-limit`]??t.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${P(this._dragged[`${l}-limit`]??t.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${e} aria-label="Remove ${a} for ${i.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(t){let e=this.room;if(t>=Et)return d`<p data-full>A room has at most ${Et} windows. Remove one to add another.</p>`;let r=this._draft,i="the new window";return d`
      <div class="draft" data-draft>
        ${this._days(r.days,i,this._onDraftDay)}
        <div class="row">
          <label for="draft-start">From</label>
          <input
            id="draft-start"
            type="time"
            data-edge="start"
            .value=${r.start}
            aria-label="Start of ${i} for ${e.name}"
            @change=${this._onDraftTime}
          />
          <label for="draft-end">Until</label>
          <input
            id="draft-end"
            type="time"
            data-edge="end"
            .value=${r.end}
            aria-label="End of ${i} for ${e.name}"
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
            aria-label="Limit of ${i} for ${e.name}"
            aria-valuetext=${P(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${P(r.limit)}</span>
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
    `}render(){let t=this.room;if(!t)return d`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let{limit:e,effectiveLimit:r,quietEnabled:i,windows:a}=t.limits,o=a.every(p=>p.start&&p.end&&p.limit!==null&&p.days.length>0),u=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c,l=e===null?"Unavailable":P(this._dragged.limit??e);return d`
      <h2>Volume limits of ${t.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${e===null?c:d`<input
              id="limit"
              type="range"
              min="0"
              max="1000"
              step="1"
              data-key="limit"
              data-server=${e}
              aria-label="Volume limit for ${t.name}"
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
        <span class="figure" data-value="effective">${r===null?"Unavailable":P(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${t.volume===null?"Unavailable":P(t.volume)}</span>
      </div>
      <h3>Quiet hours</h3>
      <div class="row">
        <button
          type="button"
          aria-label="Quiet hours for ${t.name}"
          aria-pressed=${i===!0?"true":"false"}
          ?disabled=${i===null}
          @click=${this._onEnabled}
        >
          Quiet hours
        </button>
        <span data-value="enabled">${i===null?"Unavailable":i?"On":"Off"}</span>
        <p>Off, no window caps the room, and every window is kept.</p>
      </div>
      ${a.length===0?d`<p data-none>This room has no quiet-hours window.</p>`:c}
      <ol aria-label="Quiet-hours windows of ${t.name}">
        ${a.map((p,f)=>this._window(p,f,o))}
      </ol>
      ${o?d`<h3>Add a window</h3>
            ${this._adding(a.length)}`:d`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${u}</p>
    `}};customElements.define("chorus-room-limits",re);var se="room-limits";E({id:se,path:"rooms/:room/limits",title:({room:s},t)=>`Volume limits of ${U(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>d`
    <chorus-room-limits
      .room=${U(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[ee(s)]??""}
      .refusalField=${r[ee(s)]??""}
    ></chorus-room-limits>
  `});var Or=Object.freeze(["app","kiosk"]),ie="chorus.kiosk",Er="1";function ys(s){let t=new URLSearchParams(s).get("kiosk");return t===null?null:t==="0"||t==="false"?"app":"kiosk"}function Tr(s,t){let e=ys(s);try{if(e==="kiosk")t?.setItem(ie,Er);else if(e==="app")t?.removeItem(ie);else return t?.getItem(ie)===Er?"kiosk":"app"}catch{}return e??"app"}function Cr(s=globalThis){try{return s.localStorage??null}catch{return null}}var ae=s=>`sound:${s}`,Nr=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],ws=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],ks=s=>`${s>0?"+":""}${s} dB`,oe=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(t){return this.renderRoot.querySelector(`input[data-field="${t}"]`)}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of Nr){let i=this._slider(r),a=this.room.sound[r];!i||a===null||(!this._held.has(r)||e)&&(i.value=String(a))}}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:ae(this.room.id),body:ge(this.room.id,{[t]:e})},bubbles:!0,composed:!0}))}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onSliderFocus(t){this._held.add(t.target.dataset.field)}_onSliderBlur(t){let e=t.target.dataset.field;this._held.delete(e),this._release(e);let r=this.room?.sound[e];r!=null&&(t.target.value=String(r))}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.field]:Number(t.target.value)}}_onSliderChange(t){let e=t.target.dataset.field;this._release(e),this._ask(e,Number(t.target.value))}_onSwitch(t){let e=t.currentTarget.dataset.field;this._ask(e,!this.room.sound[e])}_tone({field:t,name:e}){let r=this.room,i=r.sound[t],a=i===null?"Unavailable":ks(this._dragged[t]??i);return d`
      <div class="row">
        <label for=${t}>${e}</label>
        ${i===null?c:d`<input
              id=${t}
              data-field=${t}
              type="range"
              min=${q.min}
              max=${q.max}
              step="1"
              aria-label="${e} for ${r.name}"
              aria-valuetext=${a}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${t}>${a}</span>
      </div>
    `}_switch({field:t,name:e,says:r}){let i=this.room,a=i.sound[t];return d`
      <div class="row">
        <button
          type="button"
          data-field=${t}
          aria-label="${e} for ${i.name}"
          aria-pressed=${a===!0?"true":"false"}
          ?disabled=${a===null}
          @click=${this._onSwitch}
        >
          ${e}
        </button>
        <span data-value=${t}>${a===null?"Unavailable":a?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let t=this.room;if(!t)return d`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let e=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c;return d`
      <h2>Sound of ${t.name}</h2>
      ${Nr.map(r=>this._tone(r))} ${ws.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${e}</p>
    `}};customElements.define("chorus-room-sound",oe);var ne="room-sound";E({id:ne,path:"rooms/:room/sound",title:({room:s},t)=>`Sound of ${U(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>d`
    <chorus-room-sound
      .room=${U(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[ae(s)]??""}
      .refusalField=${r[ae(s)]??""}
    ></chorus-room-sound>
  `});var Ss={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},As=s=>`${Math.round(s/10)}%`,le=class extends b{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(r.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(he(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let r=hr(e);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(pe(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let e=t.volume===null?"Unavailable":As(this._dragged??t.volume);return d`
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
        <a href=${z(ne,{room:t.id})} data-route aria-label="Sound for ${t.name}">Sound</a>
        <a href=${z(se,{room:t.id})} data-route aria-label="Limits for ${t.name}">Limits</a>
      </div>
      ${t.bond.length===0?c:d`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(r=>d`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${Ss[r.role]??r.role}: ${r.name}
                  </li>`)}
            </ul>
          `}
      ${t.source?d`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:c}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?c:d`<input
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
          ${this.places.map(r=>d`<option value=${r.value} ?selected=${r.value===this.place}>${r.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",le);var de=class extends b{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],r=this.over;return d`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?d`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${yt(t??[],i=>i.id,i=>d`<li
              data-room=${i.id}
              data-drop="room"
              data-drop-id=${i.id}
              ?data-moving=${this.moving?.id===i.id}
              ?data-over=${r?.kind==="room"&&r.id===i.id&&this.moving?.id!==i.id}
            >
              <chorus-room-card
                .room=${i}
                .inputs=${this.inputs}
                .refusal=${this.refusals[i.id]??""}
                .places=${mr(i,t,e)}
                .place=${pr(i,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",de);var ue=class extends b{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=Ke(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",t=>this._onLink(t)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=Je({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!Z(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){Or.includes(this.mode)||(this.mode="app"),kr.includes(this.layout)||(this.layout="phone"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=Sr(t=>{this.layout=t}),this._unroute?.(),this._unroute=this._navigation.watch(t=>{t.address!==this._route.address&&(this._route=t)})}updated(t){if(!t.has("_route")||t.get("_route")===void 0)return;let e=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(e==="groups"?"section":"main");r&&(e&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!e}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""},this._refusalFields={...this._refusalFields,[t]:""};let r=await this.store.command(e);r.ok||(this._refusals={...this._refusals,[t]:r.refusal},this._refusalFields={...this._refusalFields,[t]:r.field??""})}_onCommand(t){let{subject:e,room:r,body:i,done:a}=t.detail;this._send(e??r,i).then(()=>a?.())}_move(t,e){let r=this._room(t),i=cr(r,e,this._groups);i&&this._send(t,i)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_onGo(t){let e=t.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=e,this._navigation.back();return}let r=this.renderRoot.querySelector(e==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(t){if(t.defaultPrevented||t.button>0||t.metaKey||t.ctrlKey||t.shiftKey||t.altKey)return;let e=t.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));e&&(t.preventDefault(),e.dataset.route==="back"?this._navigation.back():this._navigation.open(e.getAttribute("href")))}_screen(t){let e=jt(t.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return d`
      <main
        aria-label=${e.title(t.params,this._view)}
        data-screen=${e.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${ft} data-route="back" aria-label="Back to rooms">Back</a>
        ${e.render(t.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?c:d`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return d`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()} ${this._route.screen===mt.screen?this._home():this._screen(this._route)}
    `}_home(){return d`
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
        <a class="more" href=${z(Xt)} data-route aria-label="Autoplay rules">Autoplay</a>
        <a class="more" href=${z(Gt)} data-route aria-label="Alarms and sleep timers">Alarms</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",ue);var xs="sw.js";async function Rr(s=globalThis.navigator){let t=s?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(xs,{scope:"./",updateViaCache:"none"})}catch{return null}}function Mr({navigator:s=globalThis.navigator,document:t=globalThis.document}={}){let e=null;try{e=s?.wakeLock??null}catch{e=null}if(!e||typeof e.request!="function"||typeof t?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,i=null,a=!1,o=async l=>{try{await l.release()}catch{}},u=()=>{a||r||i||t.visibilityState!=="visible"||(i=(async()=>{try{let l=await e.request("screen");if(a){await o(l);return}r=l,l.addEventListener?.("release",()=>{r===l&&(r=null)})}catch{}finally{i=null}})())};return t.addEventListener("visibilitychange",u),u(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;i;)await i},stop:async()=>{for(a=!0,t.removeEventListener("visibilitychange",u);i;)await i;let l=r;r=null,l&&await o(l)}}}var wt=document.querySelector("chorus-app");if(wt){wt.mode=Tr(window.location.search,Cr(window)),wt.mode==="kiosk"&&Mr();let s=ir(Ae());wt.store=s,s.start()}Rr();
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
