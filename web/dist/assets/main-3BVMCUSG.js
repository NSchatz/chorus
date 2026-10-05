function Ft(s){let t=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function Vt(s,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${Ft(t)}}`}function Wt(s,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${t?"true":"false"}}`}function pt(s,t){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(t)}}`}function K(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function qt(s,t){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(t)}}`}function Kt(s,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${Ft(t)}}`}var H=Object.freeze({min:-10,max:10}),We=["bass","treble"],qe=["loudness","night","speech"];function Gt(s,t={}){let e=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of We){if(t[r]===void 0)continue;let o=Math.min(H.max,Math.max(H.min,Math.round(Number(t[r])||0)));e+=`,"${r}":${o}`}for(let r of qe)t[r]!==void 0&&(e+=`,"${r}":${t[r]?"true":"false"}`);return`${e}}`}function ft(s,t,e=""){let r=5381;for(let o of String(e))r=(Math.imul(r,33)^o.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(t)}${e?`#${r.toString(36)}`:""}`}function ht(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var Bt="Signed out";async function Ke(s){let t="";try{t=(await s.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail){let r=typeof e.field=="string"&&e.field?{field:e.field}:{};return{refusal:e.detail,...r}}}catch{}return{refusal:t||`the server answered ${s.status}`}}var Ge={set:(s,t)=>globalThis.setTimeout(s,t),clear:s=>globalThis.clearTimeout(s)};function Jt({fetch:s=globalThis.fetch.bind(globalThis),base:t="../",timers:e=Ge}={}){async function r(){let u=await s(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(ht(u))throw Object.assign(new Error(Bt),{signedOut:!0});if(!u.ok)throw new Error(`the server answered ${u.status}`);return u.json()}async function o(u){let l;try{l=await s(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:u,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(ht(l))return{ok:!1,refusal:Bt,signedOut:!0};if(!l.ok)return{ok:!1,...await Ke(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function i({onState:u,onStatus:l=()=>{}}){let f=!1,m=null,n=null,d=null,h=()=>{d!==null&&e.clear(d),d=null},v=()=>{h(),d=e.set(()=>m?.abort(),4e4)},$=b=>{let x=b.split(`
`).filter(U=>U.startsWith("data:")).map(U=>U.slice(5).replace(/^ /,"")).join(`
`);if(!x)return;let L;try{L=JSON.parse(x)}catch{return}l("live"),u(L)};async function g(){m=new AbortController,v();let b=!1;try{let x=await s(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:m.signal});if(b=ht(x),!x.ok||!x.body)throw new Error(`the server answered ${x.status}`);let L=x.body.getReader();m.signal.addEventListener("abort",()=>L.cancel().catch(()=>{}));let U=new TextDecoder,I="";for(;;){let{done:Fe,value:Ve}=await L.read();if(Fe||f||m.signal.aborted)break;v(),I+=U.decode(Ve,{stream:!0}).replace(/\r\n?/g,`
`);let ct;for(;(ct=I.indexOf(`

`))!==-1;)$(I.slice(0,ct)),I=I.slice(ct+2)}}catch{}h(),!f&&(l(b?"signed-out":"lost"),n=e.set(()=>{n=null,g()},1e3))}return g(),()=>{f=!0,h(),n!==null&&e.clear(n),m?.abort()}}return{state:r,command:o,events:i,artwork:(u,l)=>ft(t,u,l)}}var G=globalThis,J=G.ShadowRoot&&(G.ShadyCSS===void 0||G.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,mt=Symbol(),Yt=new WeakMap,D=class{constructor(t,e,r){if(this._$cssResult$=!0,r!==mt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(J&&t===void 0){let r=e!==void 0&&e.length===1;r&&(t=Yt.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),r&&Yt.set(e,t))}return t}toString(){return this.cssText}},Xt=s=>new D(typeof s=="string"?s:s+"",void 0,mt),y=(s,...t)=>{let e=s.length===1?s[0]:t.reduce((r,o,i)=>r+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(o)+s[i+1],s[0]);return new D(e,s,mt)},Qt=(s,t)=>{if(J)s.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let r=document.createElement("style"),o=G.litNonce;o!==void 0&&r.setAttribute("nonce",o),r.textContent=e.cssText,s.appendChild(r)}},gt=J?s=>s:s=>s instanceof CSSStyleSheet?(t=>{let e="";for(let r of t.cssRules)e+=r.cssText;return Xt(e)})(s):s;var{is:Je,defineProperty:Ye,getOwnPropertyDescriptor:Xe,getOwnPropertyNames:Qe,getOwnPropertySymbols:Ze,getPrototypeOf:tr}=Object,Y=globalThis,Zt=Y.trustedTypes,er=Zt?Zt.emptyScript:"",rr=Y.reactiveElementPolyfillSupport,j=(s,t)=>s,vt={toAttribute(s,t){switch(t){case Boolean:s=s?er:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,t){let e=s;switch(t){case Boolean:e=s!==null;break;case Number:e=s===null?null:Number(s);break;case Object:case Array:try{e=JSON.parse(s)}catch{e=null}}return e}},ee=(s,t)=>!Je(s,t),te={attribute:!0,type:String,converter:vt,reflect:!1,useDefault:!1,hasChanged:ee};Symbol.metadata??=Symbol("metadata"),Y.litPropertyMetadata??=new WeakMap;var w=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=te){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let r=Symbol(),o=this.getPropertyDescriptor(t,r,e);o!==void 0&&Ye(this.prototype,t,o)}}static getPropertyDescriptor(t,e,r){let{get:o,set:i}=Xe(this.prototype,t)??{get(){return this[e]},set(a){this[e]=a}};return{get:o,set(a){let u=o?.call(this);i?.call(this,a),this.requestUpdate(t,u,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??te}static _$Ei(){if(this.hasOwnProperty(j("elementProperties")))return;let t=tr(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(j("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(j("properties"))){let e=this.properties,r=[...Qe(e),...Ze(e)];for(let o of r)this.createProperty(o,e[o])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[r,o]of e)this.elementProperties.set(r,o)}this._$Eh=new Map;for(let[e,r]of this.elementProperties){let o=this._$Eu(e,r);o!==void 0&&this._$Eh.set(o,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let r=new Set(t.flat(1/0).reverse());for(let o of r)e.unshift(gt(o))}else t!==void 0&&e.push(gt(t));return e}static _$Eu(t,e){let r=e.attribute;return r===!1?void 0:typeof r=="string"?r:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let r of e.keys())this.hasOwnProperty(r)&&(t.set(r,this[r]),delete this[r]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Qt(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,r){this._$AK(t,r)}_$ET(t,e){let r=this.constructor.elementProperties.get(t),o=this.constructor._$Eu(t,r);if(o!==void 0&&r.reflect===!0){let i=(r.converter?.toAttribute!==void 0?r.converter:vt).toAttribute(e,r.type);this._$Em=t,i==null?this.removeAttribute(o):this.setAttribute(o,i),this._$Em=null}}_$AK(t,e){let r=this.constructor,o=r._$Eh.get(t);if(o!==void 0&&this._$Em!==o){let i=r.getPropertyOptions(o),a=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:vt;this._$Em=o;let u=a.fromAttribute(e,i.type);this[o]=u??this._$Ej?.get(o)??u,this._$Em=null}}requestUpdate(t,e,r,o=!1,i){if(t!==void 0){let a=this.constructor;if(o===!1&&(i=this[t]),r??=a.getPropertyOptions(t),!((r.hasChanged??ee)(i,e)||r.useDefault&&r.reflect&&i===this._$Ej?.get(t)&&!this.hasAttribute(a._$Eu(t,r))))return;this.C(t,e,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:r,reflect:o,wrapped:i},a){r&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,a??e??this[t]),i!==!0||a!==void 0)||(this._$AL.has(t)||(this.hasUpdated||r||(e=void 0),this._$AL.set(t,e)),o===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[o,i]of this._$Ep)this[o]=i;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[o,i]of r){let{wrapped:a}=i,u=this[o];a!==!0||this._$AL.has(o)||u===void 0||this.C(o,void 0,i,u)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(e)):this._$EM()}catch(r){throw t=!1,this._$EM(),r}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};w.elementStyles=[],w.shadowRootOptions={mode:"open"},w[j("elementProperties")]=new Map,w[j("finalized")]=new Map,rr?.({ReactiveElement:w}),(Y.reactiveElementVersions??=[]).push("2.1.2");var $t=globalThis,re=s=>s,X=$t.trustedTypes,se=X?X.createPolicy("lit-html",{createHTML:s=>s}):void 0,bt="$lit$",k=`lit$${Math.random().toFixed(9).slice(2)}$`,yt="?"+k,sr=`<${yt}>`,O=document,F=()=>O.createComment(""),V=s=>s===null||typeof s!="object"&&typeof s!="function",wt=Array.isArray,ue=s=>wt(s)||typeof s?.[Symbol.iterator]=="function",_t=`[ 	
\f\r]`,B=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,oe=/-->/g,ie=/>/g,E=RegExp(`>|${_t}(?:([^\\s"'>=/]+)(${_t}*=${_t}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),ne=/'/g,ae=/"/g,de=/^(?:script|style|textarea|title)$/i,kt=s=>(t,...e)=>({_$litType$:s,strings:t,values:e}),p=kt(1),Rr=kt(2),Mr=kt(3),S=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),le=new WeakMap,C=O.createTreeWalker(O,129);function ce(s,t){if(!wt(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return se!==void 0?se.createHTML(t):t}var he=(s,t)=>{let e=s.length-1,r=[],o,i=t===2?"<svg>":t===3?"<math>":"",a=B;for(let u=0;u<e;u++){let l=s[u],f,m,n=-1,d=0;for(;d<l.length&&(a.lastIndex=d,m=a.exec(l),m!==null);)d=a.lastIndex,a===B?m[1]==="!--"?a=oe:m[1]!==void 0?a=ie:m[2]!==void 0?(de.test(m[2])&&(o=RegExp("</"+m[2],"g")),a=E):m[3]!==void 0&&(a=E):a===E?m[0]===">"?(a=o??B,n=-1):m[1]===void 0?n=-2:(n=a.lastIndex-m[2].length,f=m[1],a=m[3]===void 0?E:m[3]==='"'?ae:ne):a===ae||a===ne?a=E:a===oe||a===ie?a=B:(a=E,o=void 0);let h=a===E&&s[u+1].startsWith("/>")?" ":"";i+=a===B?l+sr:n>=0?(r.push(f),l.slice(0,n)+bt+l.slice(n)+k+h):l+k+(n===-2?u:h)}return[ce(s,i+(s[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),r]},W=class s{constructor({strings:t,_$litType$:e},r){let o;this.parts=[];let i=0,a=0,u=t.length-1,l=this.parts,[f,m]=he(t,e);if(this.el=s.createElement(f,r),C.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(o=C.nextNode())!==null&&l.length<u;){if(o.nodeType===1){if(o.hasAttributes())for(let n of o.getAttributeNames())if(n.endsWith(bt)){let d=m[a++],h=o.getAttribute(n).split(k),v=/([.?@])?(.*)/.exec(d);l.push({type:1,index:i,name:v[2],strings:h,ctor:v[1]==="."?Z:v[1]==="?"?tt:v[1]==="@"?et:P}),o.removeAttribute(n)}else n.startsWith(k)&&(l.push({type:6,index:i}),o.removeAttribute(n));if(de.test(o.tagName)){let n=o.textContent.split(k),d=n.length-1;if(d>0){o.textContent=X?X.emptyScript:"";for(let h=0;h<d;h++)o.append(n[h],F()),C.nextNode(),l.push({type:2,index:++i});o.append(n[d],F())}}}else if(o.nodeType===8)if(o.data===yt)l.push({type:2,index:i});else{let n=-1;for(;(n=o.data.indexOf(k,n+1))!==-1;)l.push({type:7,index:i}),n+=k.length-1}i++}}static createElement(t,e){let r=O.createElement("template");return r.innerHTML=t,r}};function T(s,t,e=s,r){if(t===S)return t;let o=r!==void 0?e._$Co?.[r]:e._$Cl,i=V(t)?void 0:t._$litDirective$;return o?.constructor!==i&&(o?._$AO?.(!1),i===void 0?o=void 0:(o=new i(s),o._$AT(s,e,r)),r!==void 0?(e._$Co??=[])[r]=o:e._$Cl=o),o!==void 0&&(t=T(s,o._$AS(s,t.values),o,r)),t}var Q=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:r}=this._$AD,o=(t?.creationScope??O).importNode(e,!0);C.currentNode=o;let i=C.nextNode(),a=0,u=0,l=r[0];for(;l!==void 0;){if(a===l.index){let f;l.type===2?f=new R(i,i.nextSibling,this,t):l.type===1?f=new l.ctor(i,l.name,l.strings,this,t):l.type===6&&(f=new rt(i,this,t)),this._$AV.push(f),l=r[++u]}a!==l?.index&&(i=C.nextNode(),a++)}return C.currentNode=O,o}p(t){let e=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(t,r,e),e+=r.strings.length-2):r._$AI(t[e])),e++}},R=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,r,o){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=r,this.options=o,this._$Cv=o?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=T(this,t,e),V(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==S&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):ue(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&V(this._$AH)?this._$AA.nextSibling.data=t:this.T(O.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:r}=t,o=typeof r=="number"?this._$AC(t):(r.el===void 0&&(r.el=W.createElement(ce(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===o)this._$AH.p(e);else{let i=new Q(o,this),a=i.u(this.options);i.p(e),this.T(a),this._$AH=i}}_$AC(t){let e=le.get(t.strings);return e===void 0&&le.set(t.strings,e=new W(t)),e}k(t){wt(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,r,o=0;for(let i of t)o===e.length?e.push(r=new s(this.O(F()),this.O(F()),this,this.options)):r=e[o],r._$AI(i),o++;o<e.length&&(this._$AR(r&&r._$AB.nextSibling,o),e.length=o)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let r=re(t).nextSibling;re(t).remove(),t=r}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},P=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,r,o,i){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=e,this._$AM=o,this.options=i,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(t,e=this,r,o){let i=this.strings,a=!1;if(i===void 0)t=T(this,t,e,0),a=!V(t)||t!==this._$AH&&t!==S,a&&(this._$AH=t);else{let u=t,l,f;for(t=i[0],l=0;l<i.length-1;l++)f=T(this,u[r+l],e,l),f===S&&(f=this._$AH[l]),a||=!V(f)||f!==this._$AH[l],f===c?t=c:t!==c&&(t+=(f??"")+i[l+1]),this._$AH[l]=f}a&&!o&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},Z=class extends P{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},tt=class extends P{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},et=class extends P{constructor(t,e,r,o,i){super(t,e,r,o,i),this.type=5}_$AI(t,e=this){if((t=T(this,t,e,0)??c)===S)return;let r=this._$AH,o=t===c&&r!==c||t.capture!==r.capture||t.once!==r.once||t.passive!==r.passive,i=t!==c&&(r===c||o);o&&this.element.removeEventListener(this.name,this,r),i&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},rt=class{constructor(t,e,r){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(t){T(this,t)}},pe={M:bt,P:k,A:yt,C:1,L:he,R:Q,D:ue,V:T,I:R,H:P,N:tt,U:et,B:Z,F:rt},or=$t.litHtmlPolyfillSupport;or?.(W,R),($t.litHtmlVersions??=[]).push("3.3.3");var fe=(s,t,e)=>{let r=e?.renderBefore??t,o=r._$litPart$;if(o===void 0){let i=e?.renderBefore??null;r._$litPart$=o=new R(t.insertBefore(F(),i),i,void 0,e??{})}return o._$AI(s),o};var St=globalThis,_=class extends w{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=fe(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return S}};_._$litElement$=!0,_.finalized=!0,St.litElementHydrateSupport?.({LitElement:_});var ir=St.litElementPolyfillSupport;ir?.({LitElement:_});(St.litElementVersions??=[]).push("4.2.2");function nr(s,t,e){let r=s.elementFromPoint?.(t,e)??null;for(;r?.shadowRoot?.elementFromPoint;){let o=r.shadowRoot.elementFromPoint(t,e);if(!o||o===r)break;r=o}return r}function ar(s){for(let t=s;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var me=(s,t,e)=>ar(nr(s,t,e));function ge({root:s=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:r=()=>{}}={}){let o=null,i=()=>{let{handle:n,pointerId:d}=o;n.removeEventListener("pointermove",a),n.removeEventListener("pointerup",u),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),s.removeEventListener("keydown",f,!0);try{n.releasePointerCapture?.(d)}catch{}o=null};function a(n){if(!(!o||n.pointerId!==o.pointerId)){if(!o.moving){if(Math.hypot(n.clientX-o.x,n.clientY-o.y)<8)return;o.moving=!0,t(o.room)}n.preventDefault(),e(me(s,n.clientX,n.clientY))}}function u(n){if(!o||n.pointerId!==o.pointerId)return;let{room:d,moving:h}=o;if(i(),!h)return;let v=$=>{$.stopPropagation(),$.preventDefault()};s.addEventListener("click",v,!0),setTimeout(()=>s.removeEventListener("click",v,!0),0),r(d,me(s,n.clientX,n.clientY))}function l(n){if(!o||n&&n.pointerId!==void 0&&n.pointerId!==o.pointerId)return;let{room:d,moving:h}=o;i(),h&&r(d,null)}function f(n){n.key==="Escape"&&l()}function m(n){if(o||n.isPrimary===!1||n.button>0)return;let d=n.composedPath().find(h=>h.dataset?.dragRoom);if(d){o={handle:d,room:d.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{d.setPointerCapture?.(n.pointerId)}catch{}d.addEventListener("pointermove",a),d.addEventListener("pointerup",u),d.addEventListener("pointercancel",l),d.addEventListener("lostpointercapture",l),s.addEventListener("keydown",f,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!o?.moving}}function q(s,t){return t.find(e=>e.id===s.group&&e.rooms.some(r=>r.id===s.id))??null}function ve(s,t,e){if(!s||!t)return null;let r=q(s,e);return t.kind==="alone"?r?K(s.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?r&&r.id===t.id?null:pt(s.id,t.id):t.kind==="room"?t.id===s.id||r&&r.rooms.some(o=>o.id===t.id)?null:pt(s.id,t.id):null}var xt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function _e(s){if(s==="alone")return{kind:"alone"};let t=String(s).indexOf(":");if(t<1)return null;let e=s.slice(0,t),r=s.slice(t+1);return(e==="room"||e==="group")&&r?{kind:e,id:r}:null}function $e(s,t){let e=q(s,t);return e?xt({kind:"group",id:e.id}):"alone"}function be(s,t,e){return[{value:"alone",label:"Alone"},...e.map(r=>({value:xt({kind:"group",id:r.id}),label:r.name})),...t.filter(r=>r.id!==s.id&&!q(r,e)).map(r=>({value:xt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var ye={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},st=s=>(...t)=>({_$litDirective$:s,values:t}),M=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,r){this._$Ct=t,this._$AM=e,this._$Ci=r}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:lr}=pe,we=s=>s;var ke=()=>document.createComment(""),N=(s,t,e)=>{let r=s._$AA.parentNode,o=t===void 0?s._$AB:t._$AA;if(e===void 0){let i=r.insertBefore(ke(),o),a=r.insertBefore(ke(),o);e=new lr(i,a,s,s.options)}else{let i=e._$AB.nextSibling,a=e._$AM,u=a!==s;if(u){let l;e._$AQ?.(s),e._$AM=s,e._$AP!==void 0&&(l=s._$AU)!==a._$AU&&e._$AP(l)}if(i!==o||u){let l=e._$AA;for(;l!==i;){let f=we(l).nextSibling;we(r).insertBefore(l,o),l=f}}}return e},A=(s,t,e=s)=>(s._$AI(t,e),s),ur={},ot=(s,t=ur)=>s._$AH=t,Se=s=>s._$AH,it=s=>{s._$AR(),s._$AA.remove()};var xe=(s,t,e)=>{let r=new Map;for(let o=t;o<=e;o++)r.set(s[o],o);return r},nt=st(class extends M{constructor(s){if(super(s),s.type!==ye.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,t,e){let r;e===void 0?e=t:t!==void 0&&(r=t);let o=[],i=[],a=0;for(let u of s)o[a]=r?r(u,a):a,i[a]=e(u,a),a++;return{values:i,keys:o}}render(s,t,e){return this.dt(s,t,e).values}update(s,[t,e,r]){let o=Se(s),{values:i,keys:a}=this.dt(t,e,r);if(!Array.isArray(o))return this.ut=a,i;let u=this.ut??=[],l=[],f,m,n=0,d=o.length-1,h=0,v=i.length-1;for(;n<=d&&h<=v;)if(o[n]===null)n++;else if(o[d]===null)d--;else if(u[n]===a[h])l[h]=A(o[n],i[h]),n++,h++;else if(u[d]===a[v])l[v]=A(o[d],i[v]),d--,v--;else if(u[n]===a[v])l[v]=A(o[n],i[v]),N(s,l[v+1],o[n]),n++,v--;else if(u[d]===a[h])l[h]=A(o[d],i[h]),N(s,o[n],o[d]),d--,h++;else if(f===void 0&&(f=xe(a,h,v),m=xe(u,n,d)),f.has(u[n]))if(f.has(u[d])){let $=m.get(a[h]),g=$!==void 0?o[$]:null;if(g===null){let b=N(s,o[n]);A(b,i[h]),l[h]=b}else l[h]=A(g,i[h]),N(s,o[n],g),o[$]=null;h++}else it(o[d]),d--;else it(o[n]),n++;for(;h<=v;){let $=N(s,l[v+1]);A($,i[h]),l[h++]=$}for(;n<=d;){let $=o[n++];$!==null&&it($)}return this.ut=a,ot(s,l),S}});var Ae=st(class extends M{constructor(){super(...arguments),this.key=c}render(s,t){return this.key=s,t}update(s,[t,e]){return t!==this.key&&(ot(s),this.key=t),e}});var dr={playing:"Playing",paused:"Paused",buffering:"Buffering"};function cr(s,t=[]){if(!s)return"Unavailable";let e=t.find(a=>a.source===s);if(e)return e.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...o]=s.split(":"),i=o.join(":");return r==="line-in"&&i?`Input ${i}`:r==="player"&&i?`Network player ${i}`:r==="chime"&&i?`Chime ${i}`:r==="soloist"&&i?"Spotify":s}var At=class extends _{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:qt(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=p`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:Ae(t.artwork,p`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,e=this.inputs??[];return p`
      ${t?p`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?p`<p data-artist>${t.artist}</p>`:c}
              ${t.album?p`<p data-album>${t.album}</p>`:c}
              <p data-state>${dr[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${cr(this.source,e)}</p>
      ${this.pick&&e.length>0?p`<ul aria-label="Inputs for ${this.name}">
            ${e.map(r=>p`<li data-input=${r.id}>
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
    `}};customElements.define("chorus-playing",At);var hr=s=>`${Math.round(s/10)}%`,Et=class extends _{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let r=t.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Kt(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(K(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(i=>i.id)),r=t.defined??[],o=new Set(r.map(i=>i.id));return[...r.map(i=>({...i,playing:e.has(i.id)})),...t.rooms.filter(i=>!o.has(i.id)).map(i=>({...i,playing:!0}))]}render(){let t=this.group;if(!t)return c;let e=t.volume===null?"":hr(this._dragged??t.volume);return p`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?c:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(r=>p`<li data-member=${r.id} data-playing=${String(r.playing)}>
              <span>${r.name}</span>
              ${r.playing?p`<button
                    type="button"
                    aria-label="Remove ${r.name} from ${t.name}"
                    @click=${()=>this._onRemove(r)}
                  >
                    Remove
                  </button>`:p`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.source?p`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${t.kind==="live"||t.active===!0}
          ></chorus-playing>`:c}
      ${t.kind==="saved"&&!t.active?p`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:c}
      ${t.volume===null?c:p`<div class="row">
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
    `}};customElements.define("chorus-group-card",Et);var Ct=class extends _{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=y`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return p`
      ${this.groups!==null&&t.length===0?p`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${nt(t,r=>r.id,r=>p`<li
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
    `}};customElements.define("chorus-groups",Ct);var Ee=Object.freeze(["phone","desktop"]),pr=48,fr=`(min-width: ${pr}em)`;function Ce(s,t=globalThis){if(typeof t?.matchMedia!="function")return s("phone"),()=>{};let e=t.matchMedia(fr),r=()=>s(e.matches?"desktop":"phone");return e.addEventListener("change",r),r(),()=>e.removeEventListener("change",r)}var Te=Object.freeze(["app","kiosk"]),Ot="chorus.kiosk",Oe="1";function mr(s){let t=new URLSearchParams(s).get("kiosk");return t===null?null:t==="0"||t==="false"?"app":"kiosk"}function Pe(s,t){let e=mr(s);try{if(e==="kiosk")t?.setItem(Ot,Oe);else if(e==="app")t?.removeItem(Ot);else return t?.getItem(Ot)===Oe?"kiosk":"app"}catch{}return e??"app"}function Re(s=globalThis){try{return s.localStorage??null}catch{return null}}var at=[],Ne=s=>String(s).split("/").filter(Boolean);function ze(s){let{id:t,path:e,title:r,render:o}=s??{};if(typeof t!="string"||!t||t==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof o!="function")throw new Error(`the screen '${t}' has a title and a render`);let i=Ne(e);if(i.length===0)throw new Error(`the screen '${t}' has a path`);let a=u=>u.map(l=>l.startsWith(":")?":":l).join("/");for(let u of at){if(u.id===t)throw new Error(`the screen '${t}' is registered twice`);if(a(u.segments)===a(i))throw new Error(`the screens '${u.id}' and '${t}' have the same path`)}at.push({id:t,segments:i,title:r,render:o})}function Tt(s){return at.find(t=>t.id===s)??null}var ut="#/",lt=Object.freeze({screen:"home",params:Object.freeze({}),address:ut});function Pt(s,t={}){let e=Tt(s);if(!e)throw new Error(`there is no screen '${s}'`);return`#/${e.segments.map(o=>{if(!o.startsWith(":"))return o;let i=t[o.slice(1)];if(typeof i!="string"||!i)throw new Error(`the screen '${s}' needs '${o.slice(1)}'`);return encodeURIComponent(i)}).join("/")}`}function Me(s){let t;try{t=Ne(String(s??"").replace(/^#/,"")).map(e=>decodeURIComponent(e))}catch{return lt}for(let e of at){if(e.segments.length!==t.length)continue;let r={};if(e.segments.every((i,a)=>i.startsWith(":")?(r[i.slice(1)]=t[a],!0):i===t[a]))return{screen:e.id,params:r,address:Pt(e.id,r)}}return lt}function Le(s=globalThis){let t=new Set,e=()=>Me(s.location?.hash??""),r=()=>{let o=e();for(let i of[...t])i(o)};return{route:e,open(o){let i=Me(o);i.address!==e().address&&(s.history.pushState({chorus:!0},"",i.address),r())},back(){if(e().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",ut),r()}},watch(o){let i=a=>o(a);return t.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),t.add(i),i(e()),()=>{t.delete(i),t.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var gr=(s,t)=>ft("../",s,t),z=s=>typeof s=="string"&&s?s:null,vr=["playing","paused","buffering"];function Ue(s,t=gr){let e=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let o of e){if(!o||typeof o!="object"||typeof o.id!="string"||!o.id)continue;let i=o.now_playing&&typeof o.now_playing=="object"?o.now_playing:null,a=i?z(i.art_url):null;r.set(o.id,{source:z(o.source),nowPlaying:i&&{title:z(i.title),artist:z(i.artist),album:z(i.album),state:vr.includes(i.state)?i.state:null,via:z(i.via),artwork:a?t(o.id,a):null}})}return r}var Mt={source:null,nowPlaying:null};function _r(s){let t=s&&Array.isArray(s.inputs)?s.inputs:[],e=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return t.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:e.get(r)??r}))}function $r(s){let t=s&&s.sound&&typeof s.sound=="object"?s.sound:{},e=o=>Number.isInteger(o)?o:null,r=o=>typeof o=="boolean"?o:null;return{bass:e(t.bass),treble:e(t.treble),loudness:r(t.loudness),night:r(t.night),speech:r(t.speech)}}function zt(s,t){return(Array.isArray(s)?s:[]).find(e=>e.id===t)??null}function br(s,t,e){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let r=Array.isArray(s.bond)?s.bond:[],o=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:Nt(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:$r(s),group:o,...o===s.id&&e.get(o)||Mt,bond:r.filter(i=>i&&typeof i.endpoint=="string"&&typeof i.role=="string").map(i=>({endpoint:i.endpoint,name:t.get(i.endpoint)??i.endpoint,role:i.role}))}}function Ie(s,t){let e=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],o=new Map(r.filter(a=>a&&typeof a.id=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.id,a.name])),i=Ue(s,t);return e.map(a=>br(a,o,i)).filter(Boolean)}var Nt=s=>typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null;function yr(s,t){let e=new Map(Ie(s,t).map(n=>[n.id,n.name])),r=Ue(s,t),o=n=>({id:n,name:e.get(n)??n}),i=n=>Array.isArray(n)?n:[],a=n=>i(n).filter(d=>typeof d=="string"&&d).map(o),u=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=i(s?.groups).filter(u),f=i(s?.saved_groups).filter(u),m=new Set(f.map(n=>n.id));return[...f.map(n=>{let d=l.find(h=>h.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:a(n.zones),rooms:d?a(d.zones):[],volume:d?Nt(d.volume):null,...d&&r.get(n.id)||Mt}}),...l.filter(n=>n.kind==="live"&&!m.has(n.id)).map(n=>{let d=a(n.zones);return{id:n.id,name:d.map(h=>h.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:d,volume:Nt(n.volume),...r.get(n.id)??Mt}})]}var Rt=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function He(s){let t=new Set,e=null,r=[],o=[],i=[],a="connecting",u=!1,l=null,f=()=>({state:e,rooms:r,groups:o,inputs:i,status:a}),m=()=>{let g=f();for(let b of[...t])b(g)},n=g=>{e=g,r=Ie(g,s.artwork),o=yr(g,s.artwork),i=_r(g)};function d(){l||(l=s.events({onState(g){Rt(g)&&(u=!0,n(g),m())},onStatus(g){a!==g&&(a=g,m())}}),s.state().then(g=>{u||!Rt(g)||(n(g),m())},()=>{}))}function h(){l?.(),l=null}async function v(g){let b=await s.command(g);return b.signedOut&&a!=="signed-out"&&(a="signed-out",m()),b.ok&&Rt(b.state)&&(!e||b.state.serial>e.serial)&&(n(b.state),m()),b}function $(g){return t.add(g),g(f()),()=>t.delete(g)}return{start:d,stop:h,command:v,subscribe:$,view:f}}var Lt=s=>`sound:${s}`,De=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],wr=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],kr=s=>`${s>0?"+":""}${s} dB`,Ut=class extends _{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(t){return this.renderRoot.querySelector(`input[data-field="${t}"]`)}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of De){let o=this._slider(r),i=this.room.sound[r];!o||i===null||(!this._held.has(r)||e)&&(o.value=String(i))}}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Lt(this.room.id),body:Gt(this.room.id,{[t]:e})},bubbles:!0,composed:!0}))}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onSliderFocus(t){this._held.add(t.target.dataset.field)}_onSliderBlur(t){let e=t.target.dataset.field;this._held.delete(e),this._release(e);let r=this.room?.sound[e];r!=null&&(t.target.value=String(r))}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.field]:Number(t.target.value)}}_onSliderChange(t){let e=t.target.dataset.field;this._release(e),this._ask(e,Number(t.target.value))}_onSwitch(t){let e=t.currentTarget.dataset.field;this._ask(e,!this.room.sound[e])}_tone({field:t,name:e}){let r=this.room,o=r.sound[t],i=o===null?"Unavailable":kr(this._dragged[t]??o);return p`
      <div class="row">
        <label for=${t}>${e}</label>
        ${o===null?c:p`<input
              id=${t}
              data-field=${t}
              type="range"
              min=${H.min}
              max=${H.max}
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
    `}_switch({field:t,name:e,says:r}){let o=this.room,i=o.sound[t];return p`
      <div class="row">
        <button
          type="button"
          data-field=${t}
          aria-label="${e} for ${o.name}"
          aria-pressed=${i===!0?"true":"false"}
          ?disabled=${i===null}
          @click=${this._onSwitch}
        >
          ${e}
        </button>
        <span data-value=${t}>${i===null?"Unavailable":i?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let t=this.room;if(!t)return p`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let e=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c;return p`
      <h2>Sound of ${t.name}</h2>
      ${De.map(r=>this._tone(r))} ${wr.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${e}</p>
    `}};customElements.define("chorus-room-sound",Ut);var It="room-sound";ze({id:It,path:"rooms/:room/sound",title:({room:s},t)=>`Sound of ${zt(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>p`
    <chorus-room-sound
      .room=${zt(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[Lt(s)]??""}
      .refusalField=${r[Lt(s)]??""}
    ></chorus-room-sound>
  `});var Sr={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},xr=s=>`${Math.round(s/10)}%`,Ht=class extends _{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let o=t.has("refusal")&&!!this.refusal;o&&(this._dragged=null),(!this._sliderHeld||o)&&(r.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Vt(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let r=_e(e);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(Wt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let e=t.volume===null?"Unavailable":xr(this._dragged??t.volume);return p`
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
        <a href=${Pt(It,{room:t.id})} data-route aria-label="Sound for ${t.name}">Sound</a>
      </div>
      ${t.bond.length===0?c:p`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(r=>p`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${Sr[r.role]??r.role}: ${r.name}
                  </li>`)}
            </ul>
          `}
      ${t.source?p`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:c}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?c:p`<input
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
          ${this.places.map(r=>p`<option value=${r.value} ?selected=${r.value===this.place}>${r.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",Ht);var Dt=class extends _{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=y`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],r=this.over;return p`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?p`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${nt(t??[],o=>o.id,o=>p`<li
              data-room=${o.id}
              data-drop="room"
              data-drop-id=${o.id}
              ?data-moving=${this.moving?.id===o.id}
              ?data-over=${r?.kind==="room"&&r.id===o.id&&this.moving?.id!==o.id}
            >
              <chorus-room-card
                .room=${o}
                .inputs=${this.inputs}
                .refusal=${this.refusals[o.id]??""}
                .places=${be(o,t,e)}
                .place=${$e(o,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",Dt);var jt=class extends _{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=Le(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",t=>this._onLink(t)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=ge({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!q(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){Te.includes(this.mode)||(this.mode="app"),Ee.includes(this.layout)||(this.layout="phone"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=Ce(t=>{this.layout=t}),this._unroute?.(),this._unroute=this._navigation.watch(t=>{t.address!==this._route.address&&(this._route=t)})}updated(t){if(!t.has("_route")||t.get("_route")===void 0)return;let e=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(e==="groups"?"section":"main");r&&(e&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!e}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""},this._refusalFields={...this._refusalFields,[t]:""};let r=await this.store.command(e);r.ok||(this._refusals={...this._refusals,[t]:r.refusal},this._refusalFields={...this._refusalFields,[t]:r.field??""})}_onCommand(t){let{subject:e,room:r,body:o}=t.detail;this._send(e??r,o)}_move(t,e){let r=this._room(t),o=ve(r,e,this._groups);o&&this._send(t,o)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_onGo(t){let e=t.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=e,this._navigation.back();return}let r=this.renderRoot.querySelector(e==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(t){if(t.defaultPrevented||t.button>0||t.metaKey||t.ctrlKey||t.shiftKey||t.altKey)return;let e=t.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));e&&(t.preventDefault(),e.dataset.route==="back"?this._navigation.back():this._navigation.open(e.getAttribute("href")))}_screen(t){let e=Tt(t.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return p`
      <main
        aria-label=${e.title(t.params,this._view)}
        data-screen=${e.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${ut} data-route="back" aria-label="Back to rooms">Back</a>
        ${e.render(t.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?c:p`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return p`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()} ${this._route.screen===lt.screen?this._home():this._screen(this._route)}
    `}_home(){return p`
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
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",jt);var Ar="sw.js";async function je(s=globalThis.navigator){let t=s?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(Ar,{scope:"./",updateViaCache:"none"})}catch{return null}}function Be({navigator:s=globalThis.navigator,document:t=globalThis.document}={}){let e=null;try{e=s?.wakeLock??null}catch{e=null}if(!e||typeof e.request!="function"||typeof t?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,o=null,i=!1,a=async l=>{try{await l.release()}catch{}},u=()=>{i||r||o||t.visibilityState!=="visible"||(o=(async()=>{try{let l=await e.request("screen");if(i){await a(l);return}r=l,l.addEventListener?.("release",()=>{r===l&&(r=null)})}catch{}finally{o=null}})())};return t.addEventListener("visibilitychange",u),u(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;o;)await o},stop:async()=>{for(i=!0,t.removeEventListener("visibilitychange",u);o;)await o;let l=r;r=null,l&&await a(l)}}}var dt=document.querySelector("chorus-app");if(dt){dt.mode=Pe(window.location.search,Re(window)),dt.mode==="kiosk"&&Be();let s=He(Jt());dt.store=s,s.start()}je();
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
