function Z(s){let t=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function Zt(s,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${Z(t)}}`}function te(s,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${t?"true":"false"}}`}function _t(s,t){return`{"v":2,"t":"join","zone":${JSON.stringify(s)},"target":${JSON.stringify(t)}}`}function tt(s){return`{"v":2,"t":"take","target":${JSON.stringify(s)}}`}function ee(s,t){return`{"v":2,"t":"take","target":${JSON.stringify(s)},"source":${JSON.stringify(t)}}`}function re(s,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(s)},"volume":${Z(t)}}`}var W=Object.freeze({min:-10,max:10}),lr=["bass","treble"],dr=["loudness","night","speech"];function se(s,t={}){let e=`{"v":2,"t":"sound","zone":${JSON.stringify(s)}`;for(let r of lr){if(t[r]===void 0)continue;let i=Math.min(W.max,Math.max(W.min,Math.round(Number(t[r])||0)));e+=`,"${r}":${i}`}for(let r of dr)t[r]!==void 0&&(e+=`,"${r}":${t[r]?"true":"false"}`);return`${e}}`}function ie(s,t){return`{"v":2,"t":"limit","zone":${JSON.stringify(s)},"limit":${Z(t)}}`}var q=Object.freeze(["mon","tue","wed","thu","fri","sat","sun"]),yt=8;function oe(s,t=[]){let e=t.map(r=>{let i=q.filter(o=>(r.days??[]).includes(o));return`{"days":${JSON.stringify(i)},"start":${JSON.stringify(String(r.start))},"end":${JSON.stringify(String(r.end))},"limit":${Z(r.limit)}}`});return`{"v":2,"t":"quiet_hours","zone":${JSON.stringify(s)},"windows":[${e.join(",")}]}`}function ae(s,t){return`{"v":2,"t":"quiet_hours_enabled","zone":${JSON.stringify(s)},"enabled":${t?"true":"false"}}`}function ne(s,t,e,{stopOnStandby:r=!0,lowLatency:i=!0}={}){return`{"v":2,"t":"autoplay","input":${JSON.stringify(s)},"target":${JSON.stringify(t)},"enabled":${e?"true":"false"}${r===!1?',"stop_on_standby":false':""}${i===!1?',"low_latency":false':""}}`}function wt(s,t,e=""){let r=5381;for(let i of String(e))r=(Math.imul(r,33)^i.codePointAt(0))>>>0;return`${s}api/artwork?group=${encodeURIComponent(t)}${e?`#${r.toString(36)}`:""}`}function $t(s){return!!s&&(s.type==="opaqueredirect"||s.status===401)}var Xt="Signed out";async function ur(s){let t="";try{t=(await s.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail){let r=typeof e.field=="string"&&e.field?{field:e.field}:{};return{refusal:e.detail,...r}}}catch{}return{refusal:t||`the server answered ${s.status}`}}var cr={set:(s,t)=>globalThis.setTimeout(s,t),clear:s=>globalThis.clearTimeout(s)};function le({fetch:s=globalThis.fetch.bind(globalThis),base:t="../",timers:e=cr}={}){async function r(){let d=await s(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if($t(d))throw Object.assign(new Error(Xt),{signedOut:!0});if(!d.ok)throw new Error(`the server answered ${d.status}`);return d.json()}async function i(d){let l;try{l=await s(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:d,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if($t(l))return{ok:!1,refusal:Xt,signedOut:!0};if(!l.ok)return{ok:!1,...await ur(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function o({onState:d,onStatus:l=()=>{}}){let p=!1,m=null,n=null,h=null,f=()=>{h!==null&&e.clear(h),h=null},v=()=>{f(),h=e.set(()=>m?.abort(),4e4)},_=y=>{let x=y.split(`
`).filter(B=>B.startsWith("data:")).map(B=>B.slice(5).replace(/^ /,"")).join(`
`);if(!x)return;let j;try{j=JSON.parse(x)}catch{return}l("live"),d(j)};async function g(){m=new AbortController,v();let y=!1;try{let x=await s(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:m.signal});if(y=$t(x),!x.ok||!x.body)throw new Error(`the server answered ${x.status}`);let j=x.body.getReader();m.signal.addEventListener("abort",()=>j.cancel().catch(()=>{}));let B=new TextDecoder,F="";for(;;){let{done:ar,value:nr}=await j.read();if(ar||p||m.signal.aborted)break;v(),F+=B.decode(nr,{stream:!0}).replace(/\r\n?/g,`
`);let bt;for(;(bt=F.indexOf(`

`))!==-1;)_(F.slice(0,bt)),F=F.slice(bt+2)}}catch{}f(),!p&&(l(y?"signed-out":"lost"),n=e.set(()=>{n=null,g()},1e3))}return g(),()=>{p=!0,f(),n!==null&&e.clear(n),m?.abort()}}return{state:r,command:i,events:o,artwork:(d,l)=>wt(t,d,l)}}var et=globalThis,rt=et.ShadowRoot&&(et.ShadyCSS===void 0||et.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,kt=Symbol(),de=new WeakMap,V=class{constructor(t,e,r){if(this._$cssResult$=!0,r!==kt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(rt&&t===void 0){let r=e!==void 0&&e.length===1;r&&(t=de.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),r&&de.set(e,t))}return t}toString(){return this.cssText}},ue=s=>new V(typeof s=="string"?s:s+"",void 0,kt),$=(s,...t)=>{let e=s.length===1?s[0]:t.reduce((r,i,o)=>r+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(i)+s[o+1],s[0]);return new V(e,s,kt)},ce=(s,t)=>{if(rt)s.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let r=document.createElement("style"),i=et.litNonce;i!==void 0&&r.setAttribute("nonce",i),r.textContent=e.cssText,s.appendChild(r)}},St=rt?s=>s:s=>s instanceof CSSStyleSheet?(t=>{let e="";for(let r of t.cssRules)e+=r.cssText;return ue(e)})(s):s;var{is:hr,defineProperty:pr,getOwnPropertyDescriptor:fr,getOwnPropertyNames:mr,getOwnPropertySymbols:gr,getPrototypeOf:vr}=Object,st=globalThis,he=st.trustedTypes,br=he?he.emptyScript:"",$r=st.reactiveElementPolyfillSupport,J=(s,t)=>s,xt={toAttribute(s,t){switch(t){case Boolean:s=s?br:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,t){let e=s;switch(t){case Boolean:e=s!==null;break;case Number:e=s===null?null:Number(s);break;case Object:case Array:try{e=JSON.parse(s)}catch{e=null}}return e}},fe=(s,t)=>!hr(s,t),pe={attribute:!0,type:String,converter:xt,reflect:!1,useDefault:!1,hasChanged:fe};Symbol.metadata??=Symbol("metadata"),st.litPropertyMetadata??=new WeakMap;var w=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=pe){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let r=Symbol(),i=this.getPropertyDescriptor(t,r,e);i!==void 0&&pr(this.prototype,t,i)}}static getPropertyDescriptor(t,e,r){let{get:i,set:o}=fr(this.prototype,t)??{get(){return this[e]},set(a){this[e]=a}};return{get:i,set(a){let d=i?.call(this);o?.call(this,a),this.requestUpdate(t,d,r)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??pe}static _$Ei(){if(this.hasOwnProperty(J("elementProperties")))return;let t=vr(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(J("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(J("properties"))){let e=this.properties,r=[...mr(e),...gr(e)];for(let i of r)this.createProperty(i,e[i])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[r,i]of e)this.elementProperties.set(r,i)}this._$Eh=new Map;for(let[e,r]of this.elementProperties){let i=this._$Eu(e,r);i!==void 0&&this._$Eh.set(i,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let r=new Set(t.flat(1/0).reverse());for(let i of r)e.unshift(St(i))}else t!==void 0&&e.push(St(t));return e}static _$Eu(t,e){let r=e.attribute;return r===!1?void 0:typeof r=="string"?r:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let r of e.keys())this.hasOwnProperty(r)&&(t.set(r,this[r]),delete this[r]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return ce(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,r){this._$AK(t,r)}_$ET(t,e){let r=this.constructor.elementProperties.get(t),i=this.constructor._$Eu(t,r);if(i!==void 0&&r.reflect===!0){let o=(r.converter?.toAttribute!==void 0?r.converter:xt).toAttribute(e,r.type);this._$Em=t,o==null?this.removeAttribute(i):this.setAttribute(i,o),this._$Em=null}}_$AK(t,e){let r=this.constructor,i=r._$Eh.get(t);if(i!==void 0&&this._$Em!==i){let o=r.getPropertyOptions(i),a=typeof o.converter=="function"?{fromAttribute:o.converter}:o.converter?.fromAttribute!==void 0?o.converter:xt;this._$Em=i;let d=a.fromAttribute(e,o.type);this[i]=d??this._$Ej?.get(i)??d,this._$Em=null}}requestUpdate(t,e,r,i=!1,o){if(t!==void 0){let a=this.constructor;if(i===!1&&(o=this[t]),r??=a.getPropertyOptions(t),!((r.hasChanged??fe)(o,e)||r.useDefault&&r.reflect&&o===this._$Ej?.get(t)&&!this.hasAttribute(a._$Eu(t,r))))return;this.C(t,e,r)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:r,reflect:i,wrapped:o},a){r&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,a??e??this[t]),o!==!0||a!==void 0)||(this._$AL.has(t)||(this.hasUpdated||r||(e=void 0),this._$AL.set(t,e)),i===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[i,o]of this._$Ep)this[i]=o;this._$Ep=void 0}let r=this.constructor.elementProperties;if(r.size>0)for(let[i,o]of r){let{wrapped:a}=o,d=this[i];a!==!0||this._$AL.has(i)||d===void 0||this.C(i,void 0,o,d)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(r=>r.hostUpdate?.()),this.update(e)):this._$EM()}catch(r){throw t=!1,this._$EM(),r}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};w.elementStyles=[],w.shadowRootOptions={mode:"open"},w[J("elementProperties")]=new Map,w[J("finalized")]=new Map,$r?.({ReactiveElement:w}),(st.reactiveElementVersions??=[]).push("2.1.2");var Et=globalThis,me=s=>s,it=Et.trustedTypes,ge=it?it.createPolicy("lit-html",{createHTML:s=>s}):void 0,Ot="$lit$",k=`lit$${Math.random().toFixed(9).slice(2)}$`,Tt="?"+k,_r=`<${Tt}>`,T=document,Y=()=>T.createComment(""),G=s=>s===null||typeof s!="object"&&typeof s!="function",Ct=Array.isArray,we=s=>Ct(s)||typeof s?.[Symbol.iterator]=="function",At=`[ 	
\f\r]`,K=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,ve=/-->/g,be=/>/g,E=RegExp(`>|${At}(?:([^\\s"'>=/]+)(${At}*=${At}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),$e=/'/g,_e=/"/g,ke=/^(?:script|style|textarea|title)$/i,Nt=s=>(t,...e)=>({_$litType$:s,strings:t,values:e}),u=Nt(1),Zr=Nt(2),ts=Nt(3),S=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),ye=new WeakMap,O=T.createTreeWalker(T,129);function Se(s,t){if(!Ct(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return ge!==void 0?ge.createHTML(t):t}var xe=(s,t)=>{let e=s.length-1,r=[],i,o=t===2?"<svg>":t===3?"<math>":"",a=K;for(let d=0;d<e;d++){let l=s[d],p,m,n=-1,h=0;for(;h<l.length&&(a.lastIndex=h,m=a.exec(l),m!==null);)h=a.lastIndex,a===K?m[1]==="!--"?a=ve:m[1]!==void 0?a=be:m[2]!==void 0?(ke.test(m[2])&&(i=RegExp("</"+m[2],"g")),a=E):m[3]!==void 0&&(a=E):a===E?m[0]===">"?(a=i??K,n=-1):m[1]===void 0?n=-2:(n=a.lastIndex-m[2].length,p=m[1],a=m[3]===void 0?E:m[3]==='"'?_e:$e):a===_e||a===$e?a=E:a===ve||a===be?a=K:(a=E,i=void 0);let f=a===E&&s[d+1].startsWith("/>")?" ":"";o+=a===K?l+_r:n>=0?(r.push(p),l.slice(0,n)+Ot+l.slice(n)+k+f):l+k+(n===-2?d:f)}return[Se(s,o+(s[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),r]},Q=class s{constructor({strings:t,_$litType$:e},r){let i;this.parts=[];let o=0,a=0,d=t.length-1,l=this.parts,[p,m]=xe(t,e);if(this.el=s.createElement(p,r),O.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(i=O.nextNode())!==null&&l.length<d;){if(i.nodeType===1){if(i.hasAttributes())for(let n of i.getAttributeNames())if(n.endsWith(Ot)){let h=m[a++],f=i.getAttribute(n).split(k),v=/([.?@])?(.*)/.exec(h);l.push({type:1,index:o,name:v[2],strings:f,ctor:v[1]==="."?at:v[1]==="?"?nt:v[1]==="@"?lt:N}),i.removeAttribute(n)}else n.startsWith(k)&&(l.push({type:6,index:o}),i.removeAttribute(n));if(ke.test(i.tagName)){let n=i.textContent.split(k),h=n.length-1;if(h>0){i.textContent=it?it.emptyScript:"";for(let f=0;f<h;f++)i.append(n[f],Y()),O.nextNode(),l.push({type:2,index:++o});i.append(n[h],Y())}}}else if(i.nodeType===8)if(i.data===Tt)l.push({type:2,index:o});else{let n=-1;for(;(n=i.data.indexOf(k,n+1))!==-1;)l.push({type:7,index:o}),n+=k.length-1}o++}}static createElement(t,e){let r=T.createElement("template");return r.innerHTML=t,r}};function C(s,t,e=s,r){if(t===S)return t;let i=r!==void 0?e._$Co?.[r]:e._$Cl,o=G(t)?void 0:t._$litDirective$;return i?.constructor!==o&&(i?._$AO?.(!1),o===void 0?i=void 0:(i=new o(s),i._$AT(s,e,r)),r!==void 0?(e._$Co??=[])[r]=i:e._$Cl=i),i!==void 0&&(t=C(s,i._$AS(s,t.values),i,r)),t}var ot=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:r}=this._$AD,i=(t?.creationScope??T).importNode(e,!0);O.currentNode=i;let o=O.nextNode(),a=0,d=0,l=r[0];for(;l!==void 0;){if(a===l.index){let p;l.type===2?p=new R(o,o.nextSibling,this,t):l.type===1?p=new l.ctor(o,l.name,l.strings,this,t):l.type===6&&(p=new dt(o,this,t)),this._$AV.push(p),l=r[++d]}a!==l?.index&&(o=O.nextNode(),a++)}return O.currentNode=T,i}p(t){let e=0;for(let r of this._$AV)r!==void 0&&(r.strings!==void 0?(r._$AI(t,r,e),e+=r.strings.length-2):r._$AI(t[e])),e++}},R=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,r,i){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=r,this.options=i,this._$Cv=i?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=C(this,t,e),G(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==S&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):we(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&G(this._$AH)?this._$AA.nextSibling.data=t:this.T(T.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:r}=t,i=typeof r=="number"?this._$AC(t):(r.el===void 0&&(r.el=Q.createElement(Se(r.h,r.h[0]),this.options)),r);if(this._$AH?._$AD===i)this._$AH.p(e);else{let o=new ot(i,this),a=o.u(this.options);o.p(e),this.T(a),this._$AH=o}}_$AC(t){let e=ye.get(t.strings);return e===void 0&&ye.set(t.strings,e=new Q(t)),e}k(t){Ct(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,r,i=0;for(let o of t)i===e.length?e.push(r=new s(this.O(Y()),this.O(Y()),this,this.options)):r=e[i],r._$AI(o),i++;i<e.length&&(this._$AR(r&&r._$AB.nextSibling,i),e.length=i)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let r=me(t).nextSibling;me(t).remove(),t=r}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},N=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,r,i,o){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=e,this._$AM=i,this.options=o,r.length>2||r[0]!==""||r[1]!==""?(this._$AH=Array(r.length-1).fill(new String),this.strings=r):this._$AH=c}_$AI(t,e=this,r,i){let o=this.strings,a=!1;if(o===void 0)t=C(this,t,e,0),a=!G(t)||t!==this._$AH&&t!==S,a&&(this._$AH=t);else{let d=t,l,p;for(t=o[0],l=0;l<o.length-1;l++)p=C(this,d[r+l],e,l),p===S&&(p=this._$AH[l]),a||=!G(p)||p!==this._$AH[l],p===c?t=c:t!==c&&(t+=(p??"")+o[l+1]),this._$AH[l]=p}a&&!i&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},at=class extends N{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},nt=class extends N{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},lt=class extends N{constructor(t,e,r,i,o){super(t,e,r,i,o),this.type=5}_$AI(t,e=this){if((t=C(this,t,e,0)??c)===S)return;let r=this._$AH,i=t===c&&r!==c||t.capture!==r.capture||t.once!==r.once||t.passive!==r.passive,o=t!==c&&(r===c||i);i&&this.element.removeEventListener(this.name,this,r),o&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},dt=class{constructor(t,e,r){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=r}get _$AU(){return this._$AM._$AU}_$AI(t){C(this,t)}},Ae={M:Ot,P:k,A:Tt,C:1,L:xe,R:ot,D:we,V:C,I:R,H:N,N:nt,U:lt,B:at,F:dt},yr=Et.litHtmlPolyfillSupport;yr?.(Q,R),(Et.litHtmlVersions??=[]).push("3.3.3");var Ee=(s,t,e)=>{let r=e?.renderBefore??t,i=r._$litPart$;if(i===void 0){let o=e?.renderBefore??null;r._$litPart$=i=new R(t.insertBefore(Y(),o),o,void 0,e??{})}return i._$AI(s),i};var zt=globalThis,b=class extends w{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=Ee(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return S}};b._$litElement$=!0,b.finalized=!0,zt.litElementHydrateSupport?.({LitElement:b});var wr=zt.litElementPolyfillSupport;wr?.({LitElement:b});(zt.litElementVersions??=[]).push("4.2.2");function kr(s,t,e){let r=s.elementFromPoint?.(t,e)??null;for(;r?.shadowRoot?.elementFromPoint;){let i=r.shadowRoot.elementFromPoint(t,e);if(!i||i===r)break;r=i}return r}function Sr(s){for(let t=s;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var Oe=(s,t,e)=>Sr(kr(s,t,e));function Te({root:s=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:r=()=>{}}={}){let i=null,o=()=>{let{handle:n,pointerId:h}=i;n.removeEventListener("pointermove",a),n.removeEventListener("pointerup",d),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),s.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(h)}catch{}i=null};function a(n){if(!(!i||n.pointerId!==i.pointerId)){if(!i.moving){if(Math.hypot(n.clientX-i.x,n.clientY-i.y)<8)return;i.moving=!0,t(i.room)}n.preventDefault(),e(Oe(s,n.clientX,n.clientY))}}function d(n){if(!i||n.pointerId!==i.pointerId)return;let{room:h,moving:f}=i;if(o(),!f)return;let v=_=>{_.stopPropagation(),_.preventDefault()};s.addEventListener("click",v,!0),setTimeout(()=>s.removeEventListener("click",v,!0),0),r(h,Oe(s,n.clientX,n.clientY))}function l(n){if(!i||n&&n.pointerId!==void 0&&n.pointerId!==i.pointerId)return;let{room:h,moving:f}=i;o(),f&&r(h,null)}function p(n){n.key==="Escape"&&l()}function m(n){if(i||n.isPrimary===!1||n.button>0)return;let h=n.composedPath().find(f=>f.dataset?.dragRoom);if(h){i={handle:h,room:h.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{h.setPointerCapture?.(n.pointerId)}catch{}h.addEventListener("pointermove",a),h.addEventListener("pointerup",d),h.addEventListener("pointercancel",l),h.addEventListener("lostpointercapture",l),s.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!i?.moving}}function X(s,t){return t.find(e=>e.id===s.group&&e.rooms.some(r=>r.id===s.id))??null}function Ce(s,t,e){if(!s||!t)return null;let r=X(s,e);return t.kind==="alone"?r?tt(s.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?r&&r.id===t.id?null:_t(s.id,t.id):t.kind==="room"?t.id===s.id||r&&r.rooms.some(i=>i.id===t.id)?null:_t(s.id,t.id):null}var Rt=s=>s.kind==="alone"?"alone":`${s.kind}:${s.id}`;function Ne(s){if(s==="alone")return{kind:"alone"};let t=String(s).indexOf(":");if(t<1)return null;let e=s.slice(0,t),r=s.slice(t+1);return(e==="room"||e==="group")&&r?{kind:e,id:r}:null}function ze(s,t){let e=X(s,t);return e?Rt({kind:"group",id:e.id}):"alone"}function Re(s,t,e){return[{value:"alone",label:"Alone"},...e.map(r=>({value:Rt({kind:"group",id:r.id}),label:r.name})),...t.filter(r=>r.id!==s.id&&!X(r,e)).map(r=>({value:Rt({kind:"room",id:r.id}),label:`With ${r.name}`}))]}var ut=[],Le=s=>String(s).split("/").filter(Boolean);function P(s){let{id:t,path:e,title:r,render:i}=s??{};if(typeof t!="string"||!t||t==="home")throw new Error("a screen has an id, and it is not 'home'");if(typeof r!="function"||typeof i!="function")throw new Error(`the screen '${t}' has a title and a render`);let o=Le(e);if(o.length===0)throw new Error(`the screen '${t}' has a path`);let a=d=>d.map(l=>l.startsWith(":")?":":l).join("/");for(let d of ut){if(d.id===t)throw new Error(`the screen '${t}' is registered twice`);if(a(d.segments)===a(o))throw new Error(`the screens '${d.id}' and '${t}' have the same path`)}ut.push({id:t,segments:o,title:r,render:i})}function Pt(s){return ut.find(t=>t.id===s)??null}var ht="#/",ct=Object.freeze({screen:"home",params:Object.freeze({}),address:ht});function L(s,t={}){let e=Pt(s);if(!e)throw new Error(`there is no screen '${s}'`);return`#/${e.segments.map(i=>{if(!i.startsWith(":"))return i;let o=t[i.slice(1)];if(typeof o!="string"||!o)throw new Error(`the screen '${s}' needs '${i.slice(1)}'`);return encodeURIComponent(o)}).join("/")}`}function Pe(s){let t;try{t=Le(String(s??"").replace(/^#/,"")).map(e=>decodeURIComponent(e))}catch{return ct}for(let e of ut){if(e.segments.length!==t.length)continue;let r={};if(e.segments.every((o,a)=>o.startsWith(":")?(r[o.slice(1)]=t[a],!0):o===t[a]))return{screen:e.id,params:r,address:L(e.id,r)}}return ct}function Me(s=globalThis){let t=new Set,e=()=>Pe(s.location?.hash??""),r=()=>{let i=e();for(let o of[...t])o(i)};return{route:e,open(i){let o=Pe(i);o.address!==e().address&&(s.history.pushState({chorus:!0},"",o.address),r())},back(){if(e().screen!=="home"){if(s.history.state?.chorus===!0){s.history.back();return}s.history.replaceState(null,"",ht),r()}},watch(i){let o=a=>i(a);return t.size===0&&(s.addEventListener?.("popstate",r),s.addEventListener?.("hashchange",r)),t.add(o),o(e()),()=>{t.delete(o),t.size===0&&(s.removeEventListener?.("popstate",r),s.removeEventListener?.("hashchange",r))}}}}var xr=(s,t)=>wt("../",s,t),M=s=>typeof s=="string"&&s?s:null,Ar=["playing","paused","buffering"];function Ie(s,t=xr){let e=s&&Array.isArray(s.groups)?s.groups:[],r=new Map;for(let i of e){if(!i||typeof i!="object"||typeof i.id!="string"||!i.id)continue;let o=i.now_playing&&typeof i.now_playing=="object"?i.now_playing:null,a=o?M(o.art_url):null;r.set(i.id,{source:M(i.source),nowPlaying:o&&{title:M(o.title),artist:M(o.artist),album:M(o.album),state:Ar.includes(o.state)?o.state:null,via:M(o.via),artwork:a?t(i.id,a):null}})}return r}var Mt={source:null,nowPlaying:null};function Er(s){let t=s&&Array.isArray(s.inputs)?s.inputs:[],e=new Map((s&&Array.isArray(s.input_labels)?s.input_labels:[]).filter(r=>r&&typeof r.input=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.input,r.name]));return t.filter(r=>typeof r=="string"&&r).map(r=>({id:r,source:`line-in:${r}`,label:e.get(r)??r}))}function Or(s){let t=s&&s.sound&&typeof s.sound=="object"?s.sound:{},e=i=>Number.isInteger(i)?i:null,r=i=>typeof i=="boolean"?i:null;return{bass:e(t.bass),treble:e(t.treble),loudness:r(t.loudness),night:r(t.night),speech:r(t.speech)}}function Tr(s){let t=s&&typeof s=="object"?s:{},e=r=>typeof r=="string"&&/^\d\d:\d\d$/.test(r)?r:null;return{limit:I(t.limit),effectiveLimit:I(t.effective_limit),quietEnabled:typeof t.quiet_enabled=="boolean"?t.quiet_enabled:null,windows:(Array.isArray(t.quiet)?t.quiet:[]).filter(r=>r&&typeof r=="object").map(r=>({days:(Array.isArray(r.days)?r.days:[]).filter(i=>typeof i=="string"),start:e(r.start),end:e(r.end),limit:I(r.limit),active:r.active===!0}))}}function Ue(s){return(s&&Array.isArray(s.autoplay)?s.autoplay:[]).filter(e=>e&&typeof e.input=="string"&&e.input&&typeof e.target=="string").map(e=>({input:e.input,target:e.target,enabled:e.enabled===!0,stopOnStandby:e.stop_on_standby!==!1,lowLatency:e.low_latency!==!1}))}function U(s,t){return(Array.isArray(s)?s:[]).find(e=>e.id===t)??null}function Cr(s,t,e){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let r=Array.isArray(s.bond)?s.bond:[],i=typeof s.group=="string"&&s.group?s.group:s.id;return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:I(s.volume),muted:typeof s.muted=="boolean"?s.muted:null,sound:Or(s),limits:Tr(s),group:i,...i===s.id&&e.get(i)||Mt,bond:r.filter(o=>o&&typeof o.endpoint=="string"&&typeof o.role=="string").map(o=>({endpoint:o.endpoint,name:t.get(o.endpoint)??o.endpoint,role:o.role}))}}function De(s,t){let e=s&&Array.isArray(s.zones)?s.zones:[],r=s&&Array.isArray(s.speakers)?s.speakers:[],i=new Map(r.filter(a=>a&&typeof a.id=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.id,a.name])),o=Ie(s,t);return e.map(a=>Cr(a,i,o)).filter(Boolean)}function I(s){return typeof s=="number"&&s>=0&&s<=1?Math.round(s*1e3):null}function Nr(s,t){let e=new Map(De(s,t).map(n=>[n.id,n.name])),r=Ie(s,t),i=n=>({id:n,name:e.get(n)??n}),o=n=>Array.isArray(n)?n:[],a=n=>o(n).filter(h=>typeof h=="string"&&h).map(i),d=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=o(s?.groups).filter(d),p=o(s?.saved_groups).filter(d),m=new Set(p.map(n=>n.id));return[...p.map(n=>{let h=l.find(f=>f.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:a(n.zones),rooms:h?a(h.zones):[],volume:h?I(h.volume):null,...h&&r.get(n.id)||Mt}}),...l.filter(n=>n.kind==="live"&&!m.has(n.id)).map(n=>{let h=a(n.zones);return{id:n.id,name:h.map(f=>f.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:h,volume:I(n.volume),...r.get(n.id)??Mt}})]}var Lt=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function He(s){let t=new Set,e=null,r=[],i=[],o=[],a="connecting",d=!1,l=null,p=()=>({state:e,rooms:r,groups:i,inputs:o,status:a}),m=()=>{let g=p();for(let y of[...t])y(g)},n=g=>{e=g,r=De(g,s.artwork),i=Nr(g,s.artwork),o=Er(g)};function h(){l||(l=s.events({onState(g){Lt(g)&&(d=!0,n(g),m())},onStatus(g){a!==g&&(a=g,m())}}),s.state().then(g=>{d||!Lt(g)||(n(g),m())},()=>{}))}function f(){l?.(),l=null}async function v(g){let y=await s.command(g);return y.signedOut&&a!=="signed-out"&&(a="signed-out",m()),y.ok&&Lt(y.state)&&(!e||y.state.serial>e.serial)&&(n(y.state),m()),y}function _(g){return t.add(g),g(p()),()=>t.delete(g)}return{start:h,stop:f,command:v,subscribe:_,view:p}}var je=s=>`autoplay:${s}`;function zr(s){let t=Ue(s.state),e=a=>t.find(d=>d.input===a)??null,r=(s.inputs??[]).map(a=>({input:a.id,label:a.label,offered:!0,rule:e(a.id)})),i=new Set(r.map(a=>a.input)),o=new Map((Array.isArray(s.state?.input_labels)?s.state.input_labels:[]).filter(a=>a&&typeof a.input=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.input,a.name]));return[...r,...t.filter(a=>!i.has(a.input)).map(a=>({input:a.input,label:o.get(a.input)??a.input,offered:!1,rule:a}))]}var It=class extends b{static properties={rows:{attribute:!1},rooms:{attribute:!1},groups:{attribute:!1},refusals:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rows=null,this.rooms=[],this.groups=[],this.refusals={}}_row(t){return(this.rows??[]).find(e=>e.input===t)??null}_ask(t,e,r){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:je(t.input),body:ne(t.input,e,r,t.rule??{})},bubbles:!0,composed:!0}))}_onSwitch(t){let e=this._row(t.currentTarget.dataset.input);e?.rule&&this._ask(e,e.rule.target,!e.rule.enabled)}_onTarget(t){let e=this._row(t.target.dataset.input),r=t.target.value,i=e?.rule?.target??"";t.target.value=i,!(!e||!r||r===i)&&this._ask(e,r,e.rule?.enabled??!1)}updated(){for(let t of this.renderRoot.querySelectorAll("select[data-input]")){let e=this._row(t.dataset.input)?.rule?.target??"";t.value!==e&&(t.value=e)}}_targets(t){let e=this.rooms??[],r=this.groups??[],i=!t||[...e,...r].some(a=>a.id===t.target),o=a=>u`<option value=${a.id} ?selected=${t?.target===a.id}>${a.name}</option>`;return u`
      ${t?c:u`<option value="" selected>Nowhere yet</option>`}
      ${i?c:u`<option value=${t.target} selected>${t.target} (not on this server now)</option>`}
      ${e.length===0?c:u`<optgroup label="Rooms">${e.map(o)}</optgroup>`}
      ${r.length===0?c:u`<optgroup label="Saved groups">${r.map(o)}</optgroup>`}
    `}_input(t){let{input:e,label:r,offered:i,rule:o}=t,a=this.refusals?.[je(e)]??"",d=o?o.enabled?"On":"Off":"Choose where it plays, then switch it on";return u`
      <li data-input=${e}>
        <h3>${r}</h3>
        ${r===e?c:u`<p data-id>${e}</p>`}
        ${i?c:u`<p data-absent>Not offered now: its speaker is not connected.</p>`}
        <div class="row">
          <button
            type="button"
            data-input=${e}
            aria-label="Autoplay for ${r}"
            aria-pressed=${o?.enabled?"true":"false"}
            ?disabled=${!o}
            @click=${this._onSwitch}
          >
            Autoplay
          </button>
          <span data-value="enabled">${d}</span>
        </div>
        <div class="row">
          <label for="target-${e}">Plays in</label>
          <select id="target-${e}" data-input=${e} aria-label="Autoplay target for ${r}" @change=${this._onTarget}>
            ${this._targets(o)}
          </select>
        </div>
        <p role="alert">${a?`Refused: ${a}`:c}</p>
      </li>
    `}render(){return this.rows===null?u`<p role="status" data-missing>Reading this server's inputs.</p>`:u`
      <h2>Autoplay</h2>
      <p>An input with a rule that is on plays in its room or its group when its signal arrives.</p>
      ${this.rows.length===0?u`<p role="status" data-none>This server offers no input now, and has no autoplay rule.</p>`:u`<ul aria-label="Inputs">
            ${this.rows.map(t=>this._input(t))}
          </ul>`}
    `}};customElements.define("chorus-autoplay",It);var Ut="autoplay",Be=s=>s.map(({id:t,name:e})=>({id:t,name:e}));P({id:Ut,path:"autoplay",title:()=>"Autoplay",render:(s,{view:t,refusals:e})=>u`
    <chorus-autoplay
      .rows=${t.state===null?null:zr(t)}
      .rooms=${Be(t.rooms)}
      .groups=${Be((t.groups??[]).filter(r=>r.kind==="saved"))}
      .refusals=${e}
    ></chorus-autoplay>
  `});var Fe={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},pt=s=>(...t)=>({_$litDirective$:s,values:t}),D=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,r){this._$Ct=t,this._$AM=e,this._$Ci=r}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:Rr}=Ae,We=s=>s;var qe=()=>document.createComment(""),H=(s,t,e)=>{let r=s._$AA.parentNode,i=t===void 0?s._$AB:t._$AA;if(e===void 0){let o=r.insertBefore(qe(),i),a=r.insertBefore(qe(),i);e=new Rr(o,a,s,s.options)}else{let o=e._$AB.nextSibling,a=e._$AM,d=a!==s;if(d){let l;e._$AQ?.(s),e._$AM=s,e._$AP!==void 0&&(l=s._$AU)!==a._$AU&&e._$AP(l)}if(o!==i||d){let l=e._$AA;for(;l!==o;){let p=We(l).nextSibling;We(r).insertBefore(l,i),l=p}}}return e},A=(s,t,e=s)=>(s._$AI(t,e),s),Pr={},ft=(s,t=Pr)=>s._$AH=t,Ve=s=>s._$AH,mt=s=>{s._$AR(),s._$AA.remove()};var Je=(s,t,e)=>{let r=new Map;for(let i=t;i<=e;i++)r.set(s[i],i);return r},gt=pt(class extends D{constructor(s){if(super(s),s.type!==Fe.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,t,e){let r;e===void 0?e=t:t!==void 0&&(r=t);let i=[],o=[],a=0;for(let d of s)i[a]=r?r(d,a):a,o[a]=e(d,a),a++;return{values:o,keys:i}}render(s,t,e){return this.dt(s,t,e).values}update(s,[t,e,r]){let i=Ve(s),{values:o,keys:a}=this.dt(t,e,r);if(!Array.isArray(i))return this.ut=a,o;let d=this.ut??=[],l=[],p,m,n=0,h=i.length-1,f=0,v=o.length-1;for(;n<=h&&f<=v;)if(i[n]===null)n++;else if(i[h]===null)h--;else if(d[n]===a[f])l[f]=A(i[n],o[f]),n++,f++;else if(d[h]===a[v])l[v]=A(i[h],o[v]),h--,v--;else if(d[n]===a[v])l[v]=A(i[n],o[v]),H(s,l[v+1],i[n]),n++,v--;else if(d[h]===a[f])l[f]=A(i[h],o[f]),H(s,i[n],i[h]),h--,f++;else if(p===void 0&&(p=Je(a,f,v),m=Je(d,n,h)),p.has(d[n]))if(p.has(d[h])){let _=m.get(a[f]),g=_!==void 0?i[_]:null;if(g===null){let y=H(s,i[n]);A(y,o[f]),l[f]=y}else l[f]=A(g,o[f]),H(s,i[n],g),i[_]=null;f++}else mt(i[h]),h--;else mt(i[n]),n++;for(;f<=v;){let _=H(s,l[v+1]);A(_,o[f]),l[f++]=_}for(;n<=h;){let _=i[n++];_!==null&&mt(_)}return this.ut=a,ft(s,l),S}});var Ke=pt(class extends D{constructor(){super(...arguments),this.key=c}render(s,t){return this.key=s,t}update(s,[t,e]){return t!==this.key&&(ft(s),this.key=t),e}});var Lr={playing:"Playing",paused:"Paused",buffering:"Buffering"};function Mr(s,t=[]){if(!s)return"Unavailable";let e=t.find(a=>a.source===s);if(e)return e.label;if(s==="stream")return"The server's stream";if(s==="none")return"Nothing";let[r,...i]=s.split(":"),o=i.join(":");return r==="line-in"&&o?`Input ${o}`:r==="player"&&o?`Network player ${o}`:r==="chime"&&o?`Chime ${o}`:r==="soloist"&&o?"Spotify":s}var Dt=class extends b{static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:ee(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=u`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:Ke(t.artwork,u`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,e=this.inputs??[];return u`
      ${t?u`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?u`<p data-artist>${t.artist}</p>`:c}
              ${t.album?u`<p data-album>${t.album}</p>`:c}
              <p data-state>${Lr[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${Mr(this.source,e)}</p>
      ${this.pick&&e.length>0?u`<ul aria-label="Inputs for ${this.name}">
            ${e.map(r=>u`<li data-input=${r.id}>
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
    `}};customElements.define("chorus-playing",Dt);var Ir=s=>`${Math.round(s/10)}%`,Ht=class extends b{static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let r=t.has("refusal")&&!!this.refusal;r&&(this._dragged=null),(!this._sliderHeld||r)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(re(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(tt(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(o=>o.id)),r=t.defined??[],i=new Set(r.map(o=>o.id));return[...r.map(o=>({...o,playing:e.has(o.id)})),...t.rooms.filter(o=>!i.has(o.id)).map(o=>({...o,playing:!0}))]}render(){let t=this.group;if(!t)return c;let e=t.volume===null?"":Ir(this._dragged??t.volume);return u`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?c:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(r=>u`<li data-member=${r.id} data-playing=${String(r.playing)}>
              <span>${r.name}</span>
              ${r.playing?u`<button
                    type="button"
                    aria-label="Remove ${r.name} from ${t.name}"
                    @click=${()=>this._onRemove(r)}
                  >
                    Remove
                  </button>`:u`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.source?u`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${t.kind==="live"||t.active===!0}
          ></chorus-playing>`:c}
      ${t.kind==="saved"&&!t.active?u`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:c}
      ${t.volume===null?c:u`<div class="row">
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
    `}};customElements.define("chorus-group-card",Ht);var jt=class extends b{static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return u`
      ${this.groups!==null&&t.length===0?u`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${gt(t,r=>r.id,r=>u`<li
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
    `}};customElements.define("chorus-groups",jt);var Ye=Object.freeze(["phone","desktop"]),Ur=48,Dr=`(min-width: ${Ur}em)`;function Ge(s,t=globalThis){if(typeof t?.matchMedia!="function")return s("phone"),()=>{};let e=t.matchMedia(Dr),r=()=>s(e.matches?"desktop":"phone");return e.addEventListener("change",r),r(),()=>e.removeEventListener("change",r)}var Bt=s=>`limits:${s}`,Qe={mon:["Mon","Monday"],tue:["Tue","Tuesday"],wed:["Wed","Wednesday"],thu:["Thu","Thursday"],fri:["Fri","Friday"],sat:["Sat","Saturday"],sun:["Sun","Sunday"]},Hr=Object.freeze({days:q,start:"22:00",end:"07:00",limit:250}),z=s=>`${Math.round(s/10)}%`,Xe=s=>/^([01]\d|2[0-3]):[0-5]\d$/.test(s),jr=({days:s,start:t,end:e,limit:r})=>({days:s,start:t,end:e,limit:r}),Ft=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0},_draft:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._draft={...Hr},this._held=new Set,this._asked=null,this._unanswered=0}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let r of this.renderRoot.querySelectorAll("input[data-server]"))(!this._held.has(r.dataset.key)||e)&&(r.value=r.dataset.server)}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Bt(this.room.id),body:t,done:e},bubbles:!0,composed:!0}))}_askWindows(t){let e=this.room.id,r=(this._asked?.room===e?this._asked.windows:this.room.limits.windows).map(jr);t(r),this._asked={room:e,windows:r},this._unanswered+=1,this._ask(oe(e,r),()=>{this._unanswered-=1,this._unanswered===0&&(this._asked=null)})}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onFocus(t){this._held.add(t.target.dataset.key)}_onBlur(t){let{key:e,server:r}=t.target.dataset;this._held.delete(e),this._release(e),t.target.value=r}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.key]:Number(t.target.value)}}_onLimitChange(t){this._release(t.target.dataset.key),this._ask(ie(this.room.id,Number(t.target.value)))}_onEnabled(){this._ask(ae(this.room.id,!this.room.limits.quietEnabled))}_onWindowLimit(t){let e=Number(t.target.dataset.window),r=Number(t.target.value);this._release(t.target.dataset.key),this._askWindows(i=>{i[e]={...i[e],limit:r}})}_onWindowTime(t){let{window:e,edge:r,server:i}=t.target.dataset,o=t.target.value;if(!Xe(o)){t.target.value=i;return}o!==i&&this._askWindows(a=>{a[Number(e)]={...a[Number(e)],[r]:o}})}_onWindowDay(t){let{window:e,day:r}=t.currentTarget.dataset;this._askWindows(i=>{let o=i[Number(e)],a=o.days.includes(r)?o.days.filter(d=>d!==r):[...o.days,r];i[Number(e)]={...o,days:a}})}_onRemove(t){let e=Number(t.currentTarget.dataset.window);this._askWindows(r=>r.splice(e,1))}_onDraftDay(t){let e=t.currentTarget.dataset.day,r=this._draft.days.includes(e)?this._draft.days.filter(i=>i!==e):q.filter(i=>i===e||this._draft.days.includes(i));this._draft={...this._draft,days:r}}_onDraftTime(t){let e=t.target.dataset.edge;if(!Xe(t.target.value)){t.target.value=this._draft[e];return}this._draft={...this._draft,[e]:t.target.value}}_onDraftLimit(t){this._draft={...this._draft,limit:Number(t.target.value)}}_onAdd(){this._askWindows(t=>t.push({...this._draft}))}_days(t,e,r,i){let o=this.room;return u`
      <div class="row" role="group" aria-label="Days of ${e} for ${o.name}">
        ${q.map(a=>u`<button
              type="button"
              data-day=${a}
              data-window=${i??c}
              aria-label="${Qe[a][1]}, ${e} for ${o.name}"
              aria-pressed=${t.includes(a)?"true":"false"}
              @click=${r}
            >
              ${Qe[a][0]}
            </button>`)}
      </div>
    `}_window(t,e,r){let i=this.room,o=`window ${e+1}`,a=i.limits.quietEnabled!==!1,d=t.active?a?"Active now":"Inside it now, and quiet hours are off":"Not active now";if(!r)return u`<li data-window=${e}><p data-value="active">Unavailable</p></li>`;let l=`window-${e}`;return u`
      <li data-window=${e} ?data-active=${t.active}>
        <div class="row">
          <strong>Window ${e+1}</strong>
          <span data-value="active" ?data-active=${t.active&&a}>${d}</span>
        </div>
        ${this._days(t.days,o,this._onWindowDay,e)}
        <div class="row">
          <label for="${l}-start">From</label>
          <input
            id="${l}-start"
            type="time"
            data-key="${l}-start"
            data-window=${e}
            data-edge="start"
            data-server=${t.start}
            aria-label="Start of ${o} for ${i.name}"
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
            aria-label="End of ${o} for ${i.name}"
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
            aria-label="Limit of ${o} for ${i.name}"
            aria-valuetext=${z(this._dragged[`${l}-limit`]??t.limit)}
            @focus=${this._onFocus}
            @blur=${this._onBlur}
            @input=${this._onSliderInput}
            @change=${this._onWindowLimit}
          />
          <span class="figure" data-value="window-limit">${z(this._dragged[`${l}-limit`]??t.limit)}</span>
        </div>
        <div class="row">
          <button type="button" data-window=${e} aria-label="Remove ${o} for ${i.name}" @click=${this._onRemove}>
            Remove
          </button>
        </div>
      </li>
    `}_adding(t){let e=this.room;if(t>=yt)return u`<p data-full>A room has at most ${yt} windows. Remove one to add another.</p>`;let r=this._draft,i="the new window";return u`
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
            aria-valuetext=${z(r.limit)}
            @input=${this._onDraftLimit}
          />
          <span class="figure" data-value="draft-limit">${z(r.limit)}</span>
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
    `}render(){let t=this.room;if(!t)return u`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let{limit:e,effectiveLimit:r,quietEnabled:i,windows:o}=t.limits,a=o.every(p=>p.start&&p.end&&p.limit!==null&&p.days.length>0),d=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c,l=e===null?"Unavailable":z(this._dragged.limit??e);return u`
      <h2>Volume limits of ${t.name}</h2>
      <div class="row">
        <label for="limit">Volume limit</label>
        ${e===null?c:u`<input
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
        <span class="figure" data-value="effective">${r===null?"Unavailable":z(r)}</span>
        <span>Volume now</span>
        <span class="figure" data-value="volume">${t.volume===null?"Unavailable":z(t.volume)}</span>
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
      ${o.length===0?u`<p data-none>This room has no quiet-hours window.</p>`:c}
      <ol aria-label="Quiet-hours windows of ${t.name}">
        ${o.map((p,m)=>this._window(p,m,a))}
      </ol>
      ${a?u`<h3>Add a window</h3>
            ${this._adding(o.length)}`:u`<p data-unreadable>This server's windows cannot be read here, so they cannot be changed here.</p>`}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${d}</p>
    `}};customElements.define("chorus-room-limits",Ft);var Wt="room-limits";P({id:Wt,path:"rooms/:room/limits",title:({room:s},t)=>`Volume limits of ${U(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>u`
    <chorus-room-limits
      .room=${U(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[Bt(s)]??""}
      .refusalField=${r[Bt(s)]??""}
    ></chorus-room-limits>
  `});var tr=Object.freeze(["app","kiosk"]),qt="chorus.kiosk",Ze="1";function Br(s){let t=new URLSearchParams(s).get("kiosk");return t===null?null:t==="0"||t==="false"?"app":"kiosk"}function er(s,t){let e=Br(s);try{if(e==="kiosk")t?.setItem(qt,Ze);else if(e==="app")t?.removeItem(qt);else return t?.getItem(qt)===Ze?"kiosk":"app"}catch{}return e??"app"}function rr(s=globalThis){try{return s.localStorage??null}catch{return null}}var Vt=s=>`sound:${s}`,sr=[{field:"bass",name:"Bass"},{field:"treble",name:"Treble"}],Fr=[{field:"loudness",name:"Loudness",says:"Fuller bass and treble at low volume"},{field:"night",name:"Night mode",says:"Loud passages held down, quiet ones brought up"},{field:"speech",name:"Speech enhancement",says:"Voices brought forward"}],Wr=s=>`${s>0?"+":""}${s} dB`,Jt=class extends b{static properties={room:{attribute:!1},roomId:{type:String},known:{type:Boolean},refusal:{type:String},refusalField:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.roomId="",this.known=!1,this.refusal="",this.refusalField="",this._dragged={},this._held=new Set}_slider(t){return this.renderRoot.querySelector(`input[data-field="${t}"]`)}updated(t){if(!this.room)return;let e=t.has("refusal")&&!!this.refusal;e&&Object.keys(this._dragged).length>0&&(this._dragged={});for(let{field:r}of sr){let i=this._slider(r),o=this.room.sound[r];!i||o===null||(!this._held.has(r)||e)&&(i.value=String(o))}}_ask(t,e){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:Vt(this.room.id),body:se(this.room.id,{[t]:e})},bubbles:!0,composed:!0}))}_release(t){if(!(t in this._dragged))return;let{[t]:e,...r}=this._dragged;this._dragged=r}_onSliderFocus(t){this._held.add(t.target.dataset.field)}_onSliderBlur(t){let e=t.target.dataset.field;this._held.delete(e),this._release(e);let r=this.room?.sound[e];r!=null&&(t.target.value=String(r))}_onSliderInput(t){this._dragged={...this._dragged,[t.target.dataset.field]:Number(t.target.value)}}_onSliderChange(t){let e=t.target.dataset.field;this._release(e),this._ask(e,Number(t.target.value))}_onSwitch(t){let e=t.currentTarget.dataset.field;this._ask(e,!this.room.sound[e])}_tone({field:t,name:e}){let r=this.room,i=r.sound[t],o=i===null?"Unavailable":Wr(this._dragged[t]??i);return u`
      <div class="row">
        <label for=${t}>${e}</label>
        ${i===null?c:u`<input
              id=${t}
              data-field=${t}
              type="range"
              min=${W.min}
              max=${W.max}
              step="1"
              aria-label="${e} for ${r.name}"
              aria-valuetext=${o}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-value=${t}>${o}</span>
      </div>
    `}_switch({field:t,name:e,says:r}){let i=this.room,o=i.sound[t];return u`
      <div class="row">
        <button
          type="button"
          data-field=${t}
          aria-label="${e} for ${i.name}"
          aria-pressed=${o===!0?"true":"false"}
          ?disabled=${o===null}
          @click=${this._onSwitch}
        >
          ${e}
        </button>
        <span data-value=${t}>${o===null?"Unavailable":o?"On":"Off"}</span>
        <p>${r}</p>
      </div>
    `}render(){let t=this.room;if(!t)return u`<p role="status" data-missing>
        ${this.known?`This server has no room "${this.roomId}".`:"Reading this server's rooms."}
      </p>`;let e=this.refusal?`Refused${this.refusalField?` (${this.refusalField})`:""}: ${this.refusal}`:c;return u`
      <h2>Sound of ${t.name}</h2>
      ${sr.map(r=>this._tone(r))} ${Fr.map(r=>this._switch(r))}
      <p role="alert" data-refusal-field=${this.refusalField||c}>${e}</p>
    `}};customElements.define("chorus-room-sound",Jt);var Kt="room-sound";P({id:Kt,path:"rooms/:room/sound",title:({room:s},t)=>`Sound of ${U(t.rooms,s)?.name??s}`,render:({room:s},{view:t,refusals:e,refusalFields:r})=>u`
    <chorus-room-sound
      .room=${U(t.rooms,s)}
      .roomId=${s}
      .known=${t.state!==null}
      .refusal=${e[Vt(s)]??""}
      .refusalField=${r[Vt(s)]??""}
    ></chorus-room-sound>
  `});var qr={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Vr=s=>`${Math.round(s/10)}%`,Yt=class extends b{static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let r=this._slider;if(!r||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(r.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Zt(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let r=Ne(e);r&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:r},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(te(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let e=t.volume===null?"Unavailable":Vr(this._dragged??t.volume);return u`
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
        <a href=${L(Kt,{room:t.id})} data-route aria-label="Sound for ${t.name}">Sound</a>
        <a href=${L(Wt,{room:t.id})} data-route aria-label="Limits for ${t.name}">Limits</a>
      </div>
      ${t.bond.length===0?c:u`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(r=>u`<li data-endpoint=${r.endpoint} data-role=${r.role}>
                    ${qr[r.role]??r.role}: ${r.name}
                  </li>`)}
            </ul>
          `}
      ${t.source?u`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:c}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?c:u`<input
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
          ${this.places.map(r=>u`<option value=${r.value} ?selected=${r.value===this.place}>${r.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",Yt);var Gt=class extends b{static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=$`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],r=this.over;return u`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?u`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${gt(t??[],i=>i.id,i=>u`<li
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
                .places=${Re(i,t,e)}
                .place=${ze(i,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",Gt);var Qt=class extends b{static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_refusalFields:{state:!0},_route:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=$`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._refusalFields={},this._navigation=Me(),this._route=this._navigation.route(),this._unroute=null,this._goingTo=null,this.addEventListener("click",t=>this._onLink(t)),this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=Te({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!X(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){tr.includes(this.mode)||(this.mode="app"),Ye.includes(this.layout)||(this.layout="phone"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=Ge(t=>{this.layout=t}),this._unroute?.(),this._unroute=this._navigation.watch(t=>{t.address!==this._route.address&&(this._route=t)})}updated(t){if(!t.has("_route")||t.get("_route")===void 0)return;let e=this._goingTo;this._goingTo=null;let r=this.renderRoot.querySelector(e==="groups"?"section":"main");r&&(e&&r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!e}))}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._unroute?.(),this._unroute=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""},this._refusalFields={...this._refusalFields,[t]:""};let r=await this.store.command(e);r.ok||(this._refusals={...this._refusals,[t]:r.refusal},this._refusalFields={...this._refusalFields,[t]:r.field??""})}_onCommand(t){let{subject:e,room:r,body:i,done:o}=t.detail;this._send(e??r,i).then(()=>o?.())}_move(t,e){let r=this._room(t),i=Ce(r,e,this._groups);i&&this._send(t,i)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_onGo(t){let e=t.currentTarget.dataset.go;if(this._route.screen!=="home"){this._goingTo=e,this._navigation.back();return}let r=this.renderRoot.querySelector(e==="rooms"?"main":"section");r&&(r.scrollIntoView?.({block:"start"}),r.focus?.({preventScroll:!0}))}_onLink(t){if(t.defaultPrevented||t.button>0||t.metaKey||t.ctrlKey||t.shiftKey||t.altKey)return;let e=t.composedPath().find(r=>r?.localName==="a"&&r.hasAttribute("data-route"));e&&(t.preventDefault(),e.dataset.route==="back"?this._navigation.back():this._navigation.open(e.getAttribute("href")))}_screen(t){let e=Pt(t.screen),r={view:this._view,refusals:this._refusals,refusalFields:this._refusalFields};return u`
      <main
        aria-label=${e.title(t.params,this._view)}
        data-screen=${e.id}
        tabindex="-1"
        @chorus-command=${this._onCommand}
      >
        <a href=${ht} data-route="back" aria-label="Back to rooms">Back</a>
        ${e.render(t.params,r)}
      </main>
    `}_signedOut(){return this._view.status!=="signed-out"?c:u`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return u`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()} ${this._route.screen===ct.screen?this._home():this._screen(this._route)}
    `}_home(){return u`
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
        <a class="more" href=${L(Ut)} data-route aria-label="Autoplay rules">Autoplay</a>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",Qt);var Jr="sw.js";async function ir(s=globalThis.navigator){let t=s?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(Jr,{scope:"./",updateViaCache:"none"})}catch{return null}}function or({navigator:s=globalThis.navigator,document:t=globalThis.document}={}){let e=null;try{e=s?.wakeLock??null}catch{e=null}if(!e||typeof e.request!="function"||typeof t?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let r=null,i=null,o=!1,a=async l=>{try{await l.release()}catch{}},d=()=>{o||r||i||t.visibilityState!=="visible"||(i=(async()=>{try{let l=await e.request("screen");if(o){await a(l);return}r=l,l.addEventListener?.("release",()=>{r===l&&(r=null)})}catch{}finally{i=null}})())};return t.addEventListener("visibilitychange",d),d(),{supported:!0,held:()=>r!==null&&r.released!==!0,settled:async()=>{for(;i;)await i},stop:async()=>{for(o=!0,t.removeEventListener("visibilitychange",d);i;)await i;let l=r;r=null,l&&await a(l)}}}var vt=document.querySelector("chorus-app");if(vt){vt.mode=er(window.location.search,rr(window)),vt.mode==="kiosk"&&or();let s=He(le());vt.store=s,s.start()}ir();
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
