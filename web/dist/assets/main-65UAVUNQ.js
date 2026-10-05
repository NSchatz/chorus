function Rt(e){let t=Math.min(1e3,Math.max(0,Math.round(Number(e)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function Mt(e,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(e)},"volume":${Rt(t)}}`}function Nt(e,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(e)},"muted":${t?"true":"false"}}`}function lt(e,t){return`{"v":2,"t":"join","zone":${JSON.stringify(e)},"target":${JSON.stringify(t)}}`}function q(e){return`{"v":2,"t":"take","target":${JSON.stringify(e)}}`}function Lt(e,t){return`{"v":2,"t":"take","target":${JSON.stringify(e)},"source":${JSON.stringify(t)}}`}function Ut(e,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(e)},"volume":${Rt(t)}}`}function ut(e,t,r=""){let s=5381;for(let i of String(r))s=(Math.imul(s,33)^i.codePointAt(0))>>>0;return`${e}api/artwork?group=${encodeURIComponent(t)}${r?`#${s.toString(36)}`:""}`}function at(e){return!!e&&(e.type==="opaqueredirect"||e.status===401)}var Tt="Signed out";async function Ae(e){let t="";try{t=(await e.text()).trim()}catch{t=""}try{let r=JSON.parse(t);if(r&&typeof r.detail=="string"&&r.detail)return r.detail}catch{}return t||`the server answered ${e.status}`}var Se={set:(e,t)=>globalThis.setTimeout(e,t),clear:e=>globalThis.clearTimeout(e)};function Ht({fetch:e=globalThis.fetch.bind(globalThis),base:t="../",timers:r=Se}={}){async function s(){let d=await e(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(at(d))throw Object.assign(new Error(Tt),{signedOut:!0});if(!d.ok)throw new Error(`the server answered ${d.status}`);return d.json()}async function i(d){let l;try{l=await e(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:d,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(at(l))return{ok:!1,refusal:Tt,signedOut:!0};if(!l.ok)return{ok:!1,refusal:await Ae(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function o({onState:d,onStatus:l=()=>{}}){let p=!1,m=null,n=null,u=null,h=()=>{u!==null&&r.clear(u),u=null},v=()=>{h(),u=r.set(()=>m?.abort(),4e4)},_=y=>{let k=y.split(`
`).filter(H=>H.startsWith("data:")).map(H=>H.slice(5).replace(/^ /,"")).join(`
`);if(!k)return;let U;try{U=JSON.parse(k)}catch{return}l("live"),d(U)};async function g(){m=new AbortController,v();let y=!1;try{let k=await e(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:m.signal});if(y=at(k),!k.ok||!k.body)throw new Error(`the server answered ${k.status}`);let U=k.body.getReader();m.signal.addEventListener("abort",()=>U.cancel().catch(()=>{}));let H=new TextDecoder,z="";for(;;){let{done:be,value:we}=await U.read();if(be||p||m.signal.aborted)break;v(),z+=H.decode(we,{stream:!0}).replace(/\r\n?/g,`
`);let nt;for(;(nt=z.indexOf(`

`))!==-1;)_(z.slice(0,nt)),z=z.slice(nt+2)}}catch{}h(),!p&&(l(y?"signed-out":"lost"),n=r.set(()=>{n=null,g()},1e3))}return g(),()=>{p=!0,h(),n!==null&&r.clear(n),m?.abort()}}return{state:s,command:i,events:o,artwork:(d,l)=>ut(t,d,l)}}var J=globalThis,G=J.ShadowRoot&&(J.ShadyCSS===void 0||J.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,dt=Symbol(),zt=new WeakMap,D=class{constructor(t,r,s){if(this._$cssResult$=!0,s!==dt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=r}get styleSheet(){let t=this.o,r=this.t;if(G&&t===void 0){let s=r!==void 0&&r.length===1;s&&(t=zt.get(r)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),s&&zt.set(r,t))}return t}toString(){return this.cssText}},Dt=e=>new D(typeof e=="string"?e:e+"",void 0,dt),b=(e,...t)=>{let r=e.length===1?e[0]:t.reduce((s,i,o)=>s+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(i)+e[o+1],e[0]);return new D(r,e,dt)},It=(e,t)=>{if(G)e.adoptedStyleSheets=t.map(r=>r instanceof CSSStyleSheet?r:r.styleSheet);else for(let r of t){let s=document.createElement("style"),i=J.litNonce;i!==void 0&&s.setAttribute("nonce",i),s.textContent=r.cssText,e.appendChild(s)}},ct=G?e=>e:e=>e instanceof CSSStyleSheet?(t=>{let r="";for(let s of t.cssRules)r+=s.cssText;return Dt(r)})(e):e;var{is:ke,defineProperty:Ee,getOwnPropertyDescriptor:xe,getOwnPropertyNames:Ce,getOwnPropertySymbols:Pe,getPrototypeOf:Oe}=Object,Y=globalThis,jt=Y.trustedTypes,Te=jt?jt.emptyScript:"",Re=Y.reactiveElementPolyfillSupport,I=(e,t)=>e,ht={toAttribute(e,t){switch(t){case Boolean:e=e?Te:null;break;case Object:case Array:e=e==null?e:JSON.stringify(e)}return e},fromAttribute(e,t){let r=e;switch(t){case Boolean:r=e!==null;break;case Number:r=e===null?null:Number(e);break;case Object:case Array:try{r=JSON.parse(e)}catch{r=null}}return r}},Vt=(e,t)=>!ke(e,t),Bt={attribute:!0,type:String,converter:ht,reflect:!1,useDefault:!1,hasChanged:Vt};Symbol.metadata??=Symbol("metadata"),Y.litPropertyMetadata??=new WeakMap;var w=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,r=Bt){if(r.state&&(r.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((r=Object.create(r)).wrapped=!0),this.elementProperties.set(t,r),!r.noAccessor){let s=Symbol(),i=this.getPropertyDescriptor(t,s,r);i!==void 0&&Ee(this.prototype,t,i)}}static getPropertyDescriptor(t,r,s){let{get:i,set:o}=xe(this.prototype,t)??{get(){return this[r]},set(a){this[r]=a}};return{get:i,set(a){let d=i?.call(this);o?.call(this,a),this.requestUpdate(t,d,s)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??Bt}static _$Ei(){if(this.hasOwnProperty(I("elementProperties")))return;let t=Oe(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(I("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(I("properties"))){let r=this.properties,s=[...Ce(r),...Pe(r)];for(let i of s)this.createProperty(i,r[i])}let t=this[Symbol.metadata];if(t!==null){let r=litPropertyMetadata.get(t);if(r!==void 0)for(let[s,i]of r)this.elementProperties.set(s,i)}this._$Eh=new Map;for(let[r,s]of this.elementProperties){let i=this._$Eu(r,s);i!==void 0&&this._$Eh.set(i,r)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let r=[];if(Array.isArray(t)){let s=new Set(t.flat(1/0).reverse());for(let i of s)r.unshift(ct(i))}else t!==void 0&&r.push(ct(t));return r}static _$Eu(t,r){let s=r.attribute;return s===!1?void 0:typeof s=="string"?s:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,r=this.constructor.elementProperties;for(let s of r.keys())this.hasOwnProperty(s)&&(t.set(s,this[s]),delete this[s]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return It(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,r,s){this._$AK(t,s)}_$ET(t,r){let s=this.constructor.elementProperties.get(t),i=this.constructor._$Eu(t,s);if(i!==void 0&&s.reflect===!0){let o=(s.converter?.toAttribute!==void 0?s.converter:ht).toAttribute(r,s.type);this._$Em=t,o==null?this.removeAttribute(i):this.setAttribute(i,o),this._$Em=null}}_$AK(t,r){let s=this.constructor,i=s._$Eh.get(t);if(i!==void 0&&this._$Em!==i){let o=s.getPropertyOptions(i),a=typeof o.converter=="function"?{fromAttribute:o.converter}:o.converter?.fromAttribute!==void 0?o.converter:ht;this._$Em=i;let d=a.fromAttribute(r,o.type);this[i]=d??this._$Ej?.get(i)??d,this._$Em=null}}requestUpdate(t,r,s,i=!1,o){if(t!==void 0){let a=this.constructor;if(i===!1&&(o=this[t]),s??=a.getPropertyOptions(t),!((s.hasChanged??Vt)(o,r)||s.useDefault&&s.reflect&&o===this._$Ej?.get(t)&&!this.hasAttribute(a._$Eu(t,s))))return;this.C(t,r,s)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,r,{useDefault:s,reflect:i,wrapped:o},a){s&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,a??r??this[t]),o!==!0||a!==void 0)||(this._$AL.has(t)||(this.hasUpdated||s||(r=void 0),this._$AL.set(t,r)),i===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(r){Promise.reject(r)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[i,o]of this._$Ep)this[i]=o;this._$Ep=void 0}let s=this.constructor.elementProperties;if(s.size>0)for(let[i,o]of s){let{wrapped:a}=o,d=this[i];a!==!0||this._$AL.has(i)||d===void 0||this.C(i,void 0,o,d)}}let t=!1,r=this._$AL;try{t=this.shouldUpdate(r),t?(this.willUpdate(r),this._$EO?.forEach(s=>s.hostUpdate?.()),this.update(r)):this._$EM()}catch(s){throw t=!1,this._$EM(),s}t&&this._$AE(r)}willUpdate(t){}_$AE(t){this._$EO?.forEach(r=>r.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(r=>this._$ET(r,this[r])),this._$EM()}updated(t){}firstUpdated(t){}};w.elementStyles=[],w.shadowRootOptions={mode:"open"},w[I("elementProperties")]=new Map,w[I("finalized")]=new Map,Re?.({ReactiveElement:w}),(Y.reactiveElementVersions??=[]).push("2.1.2");var mt=globalThis,Ft=e=>e,K=mt.trustedTypes,Wt=K?K.createPolicy("lit-html",{createHTML:e=>e}):void 0,ft="$lit$",A=`lit$${Math.random().toFixed(9).slice(2)}$`,gt="?"+A,Me=`<${gt}>`,P=document,B=()=>P.createComment(""),V=e=>e===null||typeof e!="object"&&typeof e!="function",vt=Array.isArray,Xt=e=>vt(e)||typeof e?.[Symbol.iterator]=="function",pt=`[ 	
\f\r]`,j=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,qt=/-->/g,Jt=/>/g,x=RegExp(`>|${pt}(?:([^\\s"'>=/]+)(${pt}*=${pt}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Gt=/'/g,Yt=/"/g,Qt=/^(?:script|style|textarea|title)$/i,$t=e=>(t,...r)=>({_$litType$:e,strings:t,values:r}),f=$t(1),rr=$t(2),sr=$t(3),S=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),Kt=new WeakMap,C=P.createTreeWalker(P,129);function Zt(e,t){if(!vt(e)||!e.hasOwnProperty("raw"))throw Error("invalid template strings array");return Wt!==void 0?Wt.createHTML(t):t}var te=(e,t)=>{let r=e.length-1,s=[],i,o=t===2?"<svg>":t===3?"<math>":"",a=j;for(let d=0;d<r;d++){let l=e[d],p,m,n=-1,u=0;for(;u<l.length&&(a.lastIndex=u,m=a.exec(l),m!==null);)u=a.lastIndex,a===j?m[1]==="!--"?a=qt:m[1]!==void 0?a=Jt:m[2]!==void 0?(Qt.test(m[2])&&(i=RegExp("</"+m[2],"g")),a=x):m[3]!==void 0&&(a=x):a===x?m[0]===">"?(a=i??j,n=-1):m[1]===void 0?n=-2:(n=a.lastIndex-m[2].length,p=m[1],a=m[3]===void 0?x:m[3]==='"'?Yt:Gt):a===Yt||a===Gt?a=x:a===qt||a===Jt?a=j:(a=x,i=void 0);let h=a===x&&e[d+1].startsWith("/>")?" ":"";o+=a===j?l+Me:n>=0?(s.push(p),l.slice(0,n)+ft+l.slice(n)+A+h):l+A+(n===-2?d:h)}return[Zt(e,o+(e[r]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),s]},F=class e{constructor({strings:t,_$litType$:r},s){let i;this.parts=[];let o=0,a=0,d=t.length-1,l=this.parts,[p,m]=te(t,r);if(this.el=e.createElement(p,s),C.currentNode=this.el.content,r===2||r===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(i=C.nextNode())!==null&&l.length<d;){if(i.nodeType===1){if(i.hasAttributes())for(let n of i.getAttributeNames())if(n.endsWith(ft)){let u=m[a++],h=i.getAttribute(n).split(A),v=/([.?@])?(.*)/.exec(u);l.push({type:1,index:o,name:v[2],strings:h,ctor:v[1]==="."?Q:v[1]==="?"?Z:v[1]==="@"?tt:T}),i.removeAttribute(n)}else n.startsWith(A)&&(l.push({type:6,index:o}),i.removeAttribute(n));if(Qt.test(i.tagName)){let n=i.textContent.split(A),u=n.length-1;if(u>0){i.textContent=K?K.emptyScript:"";for(let h=0;h<u;h++)i.append(n[h],B()),C.nextNode(),l.push({type:2,index:++o});i.append(n[u],B())}}}else if(i.nodeType===8)if(i.data===gt)l.push({type:2,index:o});else{let n=-1;for(;(n=i.data.indexOf(A,n+1))!==-1;)l.push({type:7,index:o}),n+=A.length-1}o++}}static createElement(t,r){let s=P.createElement("template");return s.innerHTML=t,s}};function O(e,t,r=e,s){if(t===S)return t;let i=s!==void 0?r._$Co?.[s]:r._$Cl,o=V(t)?void 0:t._$litDirective$;return i?.constructor!==o&&(i?._$AO?.(!1),o===void 0?i=void 0:(i=new o(e),i._$AT(e,r,s)),s!==void 0?(r._$Co??=[])[s]=i:r._$Cl=i),i!==void 0&&(t=O(e,i._$AS(e,t.values),i,s)),t}var X=class{constructor(t,r){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=r}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:r},parts:s}=this._$AD,i=(t?.creationScope??P).importNode(r,!0);C.currentNode=i;let o=C.nextNode(),a=0,d=0,l=s[0];for(;l!==void 0;){if(a===l.index){let p;l.type===2?p=new R(o,o.nextSibling,this,t):l.type===1?p=new l.ctor(o,l.name,l.strings,this,t):l.type===6&&(p=new et(o,this,t)),this._$AV.push(p),l=s[++d]}a!==l?.index&&(o=C.nextNode(),a++)}return C.currentNode=P,i}p(t){let r=0;for(let s of this._$AV)s!==void 0&&(s.strings!==void 0?(s._$AI(t,s,r),r+=s.strings.length-2):s._$AI(t[r])),r++}},R=class e{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,r,s,i){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=r,this._$AM=s,this.options=i,this._$Cv=i?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,r=this._$AM;return r!==void 0&&t?.nodeType===11&&(t=r.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,r=this){t=O(this,t,r),V(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==S&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):Xt(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&V(this._$AH)?this._$AA.nextSibling.data=t:this.T(P.createTextNode(t)),this._$AH=t}$(t){let{values:r,_$litType$:s}=t,i=typeof s=="number"?this._$AC(t):(s.el===void 0&&(s.el=F.createElement(Zt(s.h,s.h[0]),this.options)),s);if(this._$AH?._$AD===i)this._$AH.p(r);else{let o=new X(i,this),a=o.u(this.options);o.p(r),this.T(a),this._$AH=o}}_$AC(t){let r=Kt.get(t.strings);return r===void 0&&Kt.set(t.strings,r=new F(t)),r}k(t){vt(this._$AH)||(this._$AH=[],this._$AR());let r=this._$AH,s,i=0;for(let o of t)i===r.length?r.push(s=new e(this.O(B()),this.O(B()),this,this.options)):s=r[i],s._$AI(o),i++;i<r.length&&(this._$AR(s&&s._$AB.nextSibling,i),r.length=i)}_$AR(t=this._$AA.nextSibling,r){for(this._$AP?.(!1,!0,r);t!==this._$AB;){let s=Ft(t).nextSibling;Ft(t).remove(),t=s}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},T=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,r,s,i,o){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=r,this._$AM=i,this.options=o,s.length>2||s[0]!==""||s[1]!==""?(this._$AH=Array(s.length-1).fill(new String),this.strings=s):this._$AH=c}_$AI(t,r=this,s,i){let o=this.strings,a=!1;if(o===void 0)t=O(this,t,r,0),a=!V(t)||t!==this._$AH&&t!==S,a&&(this._$AH=t);else{let d=t,l,p;for(t=o[0],l=0;l<o.length-1;l++)p=O(this,d[s+l],r,l),p===S&&(p=this._$AH[l]),a||=!V(p)||p!==this._$AH[l],p===c?t=c:t!==c&&(t+=(p??"")+o[l+1]),this._$AH[l]=p}a&&!i&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},Q=class extends T{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},Z=class extends T{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},tt=class extends T{constructor(t,r,s,i,o){super(t,r,s,i,o),this.type=5}_$AI(t,r=this){if((t=O(this,t,r,0)??c)===S)return;let s=this._$AH,i=t===c&&s!==c||t.capture!==s.capture||t.once!==s.once||t.passive!==s.passive,o=t!==c&&(s===c||i);i&&this.element.removeEventListener(this.name,this,s),o&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},et=class{constructor(t,r,s){this.element=t,this.type=6,this._$AN=void 0,this._$AM=r,this.options=s}get _$AU(){return this._$AM._$AU}_$AI(t){O(this,t)}},ee={M:ft,P:A,A:gt,C:1,L:te,R:X,D:Xt,V:O,I:R,H:T,N:Z,U:tt,B:Q,F:et},Ne=mt.litHtmlPolyfillSupport;Ne?.(F,R),(mt.litHtmlVersions??=[]).push("3.3.3");var re=(e,t,r)=>{let s=r?.renderBefore??t,i=s._$litPart$;if(i===void 0){let o=r?.renderBefore??null;s._$litPart$=i=new R(t.insertBefore(B(),o),o,void 0,r??{})}return i._$AI(e),i};var _t=globalThis,$=class extends w{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let r=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=re(r,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return S}};$._$litElement$=!0,$.finalized=!0,_t.litElementHydrateSupport?.({LitElement:$});var Le=_t.litElementPolyfillSupport;Le?.({LitElement:$});(_t.litElementVersions??=[]).push("4.2.2");function Ue(e,t,r){let s=e.elementFromPoint?.(t,r)??null;for(;s?.shadowRoot?.elementFromPoint;){let i=s.shadowRoot.elementFromPoint(t,r);if(!i||i===s)break;s=i}return s}function He(e){for(let t=e;t;t=t.assignedSlot??t.parentNode??t.host){let r=t.dataset?.drop;if(r==="alone")return{kind:r};if((r==="room"||r==="group")&&t.dataset.dropId)return{kind:r,id:t.dataset.dropId}}return null}var se=(e,t,r)=>He(Ue(e,t,r));function ie({root:e=document,onStart:t=()=>{},onOver:r=()=>{},onEnd:s=()=>{}}={}){let i=null,o=()=>{let{handle:n,pointerId:u}=i;n.removeEventListener("pointermove",a),n.removeEventListener("pointerup",d),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),e.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(u)}catch{}i=null};function a(n){if(!(!i||n.pointerId!==i.pointerId)){if(!i.moving){if(Math.hypot(n.clientX-i.x,n.clientY-i.y)<8)return;i.moving=!0,t(i.room)}n.preventDefault(),r(se(e,n.clientX,n.clientY))}}function d(n){if(!i||n.pointerId!==i.pointerId)return;let{room:u,moving:h}=i;if(o(),!h)return;let v=_=>{_.stopPropagation(),_.preventDefault()};e.addEventListener("click",v,!0),setTimeout(()=>e.removeEventListener("click",v,!0),0),s(u,se(e,n.clientX,n.clientY))}function l(n){if(!i||n&&n.pointerId!==void 0&&n.pointerId!==i.pointerId)return;let{room:u,moving:h}=i;o(),h&&s(u,null)}function p(n){n.key==="Escape"&&l()}function m(n){if(i||n.isPrimary===!1||n.button>0)return;let u=n.composedPath().find(h=>h.dataset?.dragRoom);if(u){i={handle:u,room:u.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{u.setPointerCapture?.(n.pointerId)}catch{}u.addEventListener("pointermove",a),u.addEventListener("pointerup",d),u.addEventListener("pointercancel",l),u.addEventListener("lostpointercapture",l),e.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!i?.moving}}function W(e,t){return t.find(r=>r.id===e.group&&r.rooms.some(s=>s.id===e.id))??null}function oe(e,t,r){if(!e||!t)return null;let s=W(e,r);return t.kind==="alone"?s?q(e.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?s&&s.id===t.id?null:lt(e.id,t.id):t.kind==="room"?t.id===e.id||s&&s.rooms.some(i=>i.id===t.id)?null:lt(e.id,t.id):null}var yt=e=>e.kind==="alone"?"alone":`${e.kind}:${e.id}`;function ne(e){if(e==="alone")return{kind:"alone"};let t=String(e).indexOf(":");if(t<1)return null;let r=e.slice(0,t),s=e.slice(t+1);return(r==="room"||r==="group")&&s?{kind:r,id:s}:null}function ae(e,t){let r=W(e,t);return r?yt({kind:"group",id:r.id}):"alone"}function le(e,t,r){return[{value:"alone",label:"Alone"},...r.map(s=>({value:yt({kind:"group",id:s.id}),label:s.name})),...t.filter(s=>s.id!==e.id&&!W(s,r)).map(s=>({value:yt({kind:"room",id:s.id}),label:`With ${s.name}`}))]}var ue={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},rt=e=>(...t)=>({_$litDirective$:e,values:t}),M=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,r,s){this._$Ct=t,this._$AM=r,this._$Ci=s}_$AS(t,r){return this.update(t,r)}update(t,r){return this.render(...r)}};var{I:ze}=ee,de=e=>e;var ce=()=>document.createComment(""),N=(e,t,r)=>{let s=e._$AA.parentNode,i=t===void 0?e._$AB:t._$AA;if(r===void 0){let o=s.insertBefore(ce(),i),a=s.insertBefore(ce(),i);r=new ze(o,a,e,e.options)}else{let o=r._$AB.nextSibling,a=r._$AM,d=a!==e;if(d){let l;r._$AQ?.(e),r._$AM=e,r._$AP!==void 0&&(l=e._$AU)!==a._$AU&&r._$AP(l)}if(o!==i||d){let l=r._$AA;for(;l!==o;){let p=de(l).nextSibling;de(s).insertBefore(l,i),l=p}}}return r},E=(e,t,r=e)=>(e._$AI(t,r),e),De={},st=(e,t=De)=>e._$AH=t,he=e=>e._$AH,it=e=>{e._$AR(),e._$AA.remove()};var pe=(e,t,r)=>{let s=new Map;for(let i=t;i<=r;i++)s.set(e[i],i);return s},ot=rt(class extends M{constructor(e){if(super(e),e.type!==ue.CHILD)throw Error("repeat() can only be used in text expressions")}dt(e,t,r){let s;r===void 0?r=t:t!==void 0&&(s=t);let i=[],o=[],a=0;for(let d of e)i[a]=s?s(d,a):a,o[a]=r(d,a),a++;return{values:o,keys:i}}render(e,t,r){return this.dt(e,t,r).values}update(e,[t,r,s]){let i=he(e),{values:o,keys:a}=this.dt(t,r,s);if(!Array.isArray(i))return this.ut=a,o;let d=this.ut??=[],l=[],p,m,n=0,u=i.length-1,h=0,v=o.length-1;for(;n<=u&&h<=v;)if(i[n]===null)n++;else if(i[u]===null)u--;else if(d[n]===a[h])l[h]=E(i[n],o[h]),n++,h++;else if(d[u]===a[v])l[v]=E(i[u],o[v]),u--,v--;else if(d[n]===a[v])l[v]=E(i[n],o[v]),N(e,l[v+1],i[n]),n++,v--;else if(d[u]===a[h])l[h]=E(i[u],o[h]),N(e,i[n],i[u]),u--,h++;else if(p===void 0&&(p=pe(a,h,v),m=pe(d,n,u)),p.has(d[n]))if(p.has(d[u])){let _=m.get(a[h]),g=_!==void 0?i[_]:null;if(g===null){let y=N(e,i[n]);E(y,o[h]),l[h]=y}else l[h]=E(g,o[h]),N(e,i[n],g),i[_]=null;h++}else it(i[u]),u--;else it(i[n]),n++;for(;h<=v;){let _=N(e,l[v+1]);E(_,o[h]),l[h++]=_}for(;n<=u;){let _=i[n++];_!==null&&it(_)}return this.ut=a,st(e,l),S}});var me=rt(class extends M{constructor(){super(...arguments),this.key=c}render(e,t){return this.key=e,t}update(e,[t,r]){return t!==this.key&&(st(e),this.key=t),r}});var Ie={playing:"Playing",paused:"Paused",buffering:"Buffering"};function je(e,t=[]){if(!e)return"Unavailable";let r=t.find(a=>a.source===e);if(r)return r.label;if(e==="stream")return"The server's stream";if(e==="none")return"Nothing";let[s,...i]=e.split(":"),o=i.join(":");return s==="line-in"&&o?`Input ${o}`:s==="player"&&o?`Network player ${o}`:s==="chime"&&o?`Chime ${o}`:s==="soloist"&&o?"Spotify":e}var bt=class extends ${static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:Lt(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let r=f`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?r:me(t.artwork,f`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,r=this.inputs??[];return f`
      ${t?f`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?f`<p data-artist>${t.artist}</p>`:c}
              ${t.album?f`<p data-album>${t.album}</p>`:c}
              <p data-state>${Ie[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${je(this.source,r)}</p>
      ${this.pick&&r.length>0?f`<ul aria-label="Inputs for ${this.name}">
            ${r.map(s=>f`<li data-input=${s.id}>
                  <button
                    type="button"
                    aria-label="Play ${s.label} in ${this.name}"
                    aria-pressed=${s.source===this.source?"true":"false"}
                    @click=${()=>this._onInput(s)}
                  >
                    ${s.label}
                  </button>
                </li>`)}
          </ul>`:c}
    `}};customElements.define("chorus-playing",bt);var Be=e=>`${Math.round(e/10)}%`,wt=class extends ${static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=b`
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
    input[type="range"] {
      flex: 1;
      min-width: var(--shrink-min);
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let r=this._slider;if(!r||!this.group||this.group.volume===null)return;let s=t.has("refusal")&&!!this.refusal;s&&(this._dragged=null),(!this._sliderHeld||s)&&(r.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Ut(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(q(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,r=new Set(t.rooms.map(o=>o.id)),s=t.defined??[],i=new Set(s.map(o=>o.id));return[...s.map(o=>({...o,playing:r.has(o.id)})),...t.rooms.filter(o=>!i.has(o.id)).map(o=>({...o,playing:!0}))]}render(){let t=this.group;if(!t)return c;let r=t.volume===null?"":Be(this._dragged??t.volume);return f`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?c:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(s=>f`<li data-member=${s.id} data-playing=${String(s.playing)}>
              <span>${s.name}</span>
              ${s.playing?f`<button
                    type="button"
                    aria-label="Remove ${s.name} from ${t.name}"
                    @click=${()=>this._onRemove(s)}
                  >
                    Remove
                  </button>`:f`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.source?f`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            ?pick=${t.kind==="live"||t.active===!0}
          ></chorus-playing>`:c}
      ${t.kind==="saved"&&!t.active?f`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:c}
      ${t.volume===null?c:f`<div class="row">
            <label for="volume">Group volume</label>
            <input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Group volume for ${t.name}"
              aria-valuetext=${r}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />
            <span class="figure" data-volume>${r}</span>
          </div>`}
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-group-card",wt);var At=class extends ${static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],r=this.over;return f`
      ${this.groups!==null&&t.length===0?f`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${ot(t,s=>s.id,s=>f`<li
              data-group=${s.id}
              data-drop="group"
              data-drop-id=${s.id}
              ?data-over=${r?.kind==="group"&&r.id===s.id}
            >
              <chorus-group-card
                .group=${s}
                .inputs=${this.inputs}
                .refusal=${this.refusals[s.id]??""}
              ></chorus-group-card>
            </li>`)}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${r?.kind==="alone"}>
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:c}
      </p>
    `}};customElements.define("chorus-groups",At);var fe=Object.freeze(["app","kiosk"]);function ge(e){let t=new URLSearchParams(e).get("kiosk");return t===null||t==="0"||t==="false"?"app":"kiosk"}var Ve={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Fe=e=>`${Math.round(e/10)}%`,St=class extends ${static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=b`
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
    input[type="range"] {
      flex: 1;
      min-width: var(--shrink-min);
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
    select:focus-visible,
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      margin: var(--reset-margin);
      color: var(--bad);
    }
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let r=this._list;r&&(r.value=this.place);let s=this._slider;if(!s||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(s.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Mt(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let r=t.target.value;if(t.target.value=this.place,r===this.place)return;let s=ne(r);s&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:s},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(Nt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let r=t.volume===null?"Unavailable":Fe(this._dragged??t.volume);return f`
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
      </div>
      ${t.bond.length===0?c:f`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(s=>f`<li data-endpoint=${s.endpoint} data-role=${s.role}>
                    ${Ve[s.role]??s.role}: ${s.name}
                  </li>`)}
            </ul>
          `}
      ${t.source?f`<chorus-playing
            .target=${t.id}
            .name=${t.name}
            .source=${t.source}
            .nowPlaying=${t.nowPlaying}
            .inputs=${this.inputs}
            pick
          ></chorus-playing>`:c}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?c:f`<input
              id="volume"
              type="range"
              min="0"
              max="1000"
              step="1"
              aria-label="Volume for ${t.name}"
              aria-valuetext=${r}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />`}
        <span class="figure" data-volume>${r}</span>
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
          ${this.places.map(s=>f`<option value=${s.value} ?selected=${s.value===this.place}>${s.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",St);var kt=class extends ${static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,r=this.groups??[],s=this.over;return f`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?f`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${ot(t??[],i=>i.id,i=>f`<li
              data-room=${i.id}
              data-drop="room"
              data-drop-id=${i.id}
              ?data-moving=${this.moving?.id===i.id}
              ?data-over=${s?.kind==="room"&&s.id===i.id&&this.moving?.id!==i.id}
            >
              <chorus-room-card
                .room=${i}
                .inputs=${this.inputs}
                .refusal=${this.refusals[i.id]??""}
                .places=${le(i,t,r)}
                .place=${ae(i,r)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",kt);var Et=class extends ${static properties={mode:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=b`
    :host {
      display: block;
    }
    header {
      display: flex;
      align-items: baseline;
      gap: var(--surface-gap);
      padding: var(--surface-pad);
      border-bottom: var(--stroke-1) solid var(--border);
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
    /* A wall tablet shows the rooms and nothing of the app around them. */
    :host([mode="kiosk"]) header {
      display: none;
    }
  `;constructor(){super(),this.mode="app",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._moving=null,this._over=null,this._unsubscribe=null,this._drag=ie({onStart:t=>{let r=this._room(t);r&&(this._moving={id:t,name:r.name,grouped:!!W(r,this._groups)})},onOver:t=>{let r=this._over;r?.kind===t?.kind&&r?.id===t?.id||(this._over=t)},onEnd:(t,r)=>{this._moving=null,this._over=null,r&&this._move(t,r)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(r=>r.id===t)??null}willUpdate(t){fe.includes(this.mode)||(this.mode="app"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow()}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,r){if(!this.store)return;this._refusals={...this._refusals,[t]:""};let s=await this.store.command(r);s.ok||(this._refusals={...this._refusals,[t]:s.refusal})}_onCommand(t){let{subject:r,room:s,body:i}=t.detail;this._send(r??s,i)}_move(t,r){let s=this._room(t),i=oe(s,r,this._groups);i&&this._send(t,i)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_signedOut(){return this._view.status!=="signed-out"?c:f`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return f`
      <header>
        <h1>chorus</h1>
      </header>
      ${this._signedOut()}
      <section
        aria-label="Groups"
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
    `}};customElements.define("chorus-app",Et);var We="sw.js";async function ve(e=globalThis.navigator){let t=e?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(We,{scope:"./",updateViaCache:"none"})}catch{return null}}var qe=(e,t)=>ut("../",e,t),L=e=>typeof e=="string"&&e?e:null,Je=["playing","paused","buffering"];function $e(e,t=qe){let r=e&&Array.isArray(e.groups)?e.groups:[],s=new Map;for(let i of r){if(!i||typeof i!="object"||typeof i.id!="string"||!i.id)continue;let o=i.now_playing&&typeof i.now_playing=="object"?i.now_playing:null,a=o?L(o.art_url):null;s.set(i.id,{source:L(i.source),nowPlaying:o&&{title:L(o.title),artist:L(o.artist),album:L(o.album),state:Je.includes(o.state)?o.state:null,via:L(o.via),artwork:a?t(i.id,a):null}})}return s}var Ct={source:null,nowPlaying:null};function Ge(e){let t=e&&Array.isArray(e.inputs)?e.inputs:[],r=new Map((e&&Array.isArray(e.input_labels)?e.input_labels:[]).filter(s=>s&&typeof s.input=="string"&&typeof s.name=="string"&&s.name).map(s=>[s.input,s.name]));return t.filter(s=>typeof s=="string"&&s).map(s=>({id:s,source:`line-in:${s}`,label:r.get(s)??s}))}function Ye(e,t,r){if(!e||typeof e!="object"||typeof e.id!="string"||!e.id)return null;let s=Array.isArray(e.bond)?e.bond:[],i=typeof e.group=="string"&&e.group?e.group:e.id;return{id:e.id,name:typeof e.name=="string"&&e.name?e.name:e.id,volume:Pt(e.volume),muted:typeof e.muted=="boolean"?e.muted:null,group:i,...i===e.id&&r.get(i)||Ct,bond:s.filter(o=>o&&typeof o.endpoint=="string"&&typeof o.role=="string").map(o=>({endpoint:o.endpoint,name:t.get(o.endpoint)??o.endpoint,role:o.role}))}}function _e(e,t){let r=e&&Array.isArray(e.zones)?e.zones:[],s=e&&Array.isArray(e.speakers)?e.speakers:[],i=new Map(s.filter(a=>a&&typeof a.id=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.id,a.name])),o=$e(e,t);return r.map(a=>Ye(a,i,o)).filter(Boolean)}var Pt=e=>typeof e=="number"&&e>=0&&e<=1?Math.round(e*1e3):null;function Ke(e,t){let r=new Map(_e(e,t).map(n=>[n.id,n.name])),s=$e(e,t),i=n=>({id:n,name:r.get(n)??n}),o=n=>Array.isArray(n)?n:[],a=n=>o(n).filter(u=>typeof u=="string"&&u).map(i),d=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=o(e?.groups).filter(d),p=o(e?.saved_groups).filter(d),m=new Set(p.map(n=>n.id));return[...p.map(n=>{let u=l.find(h=>h.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:a(n.zones),rooms:u?a(u.zones):[],volume:u?Pt(u.volume):null,...u&&s.get(n.id)||Ct}}),...l.filter(n=>n.kind==="live"&&!m.has(n.id)).map(n=>{let u=a(n.zones);return{id:n.id,name:u.map(h=>h.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:u,volume:Pt(n.volume),...s.get(n.id)??Ct}})]}var xt=e=>!!e&&typeof e=="object"&&Array.isArray(e.zones);function ye(e){let t=new Set,r=null,s=[],i=[],o=[],a="connecting",d=!1,l=null,p=()=>({state:r,rooms:s,groups:i,inputs:o,status:a}),m=()=>{let g=p();for(let y of[...t])y(g)},n=g=>{r=g,s=_e(g,e.artwork),i=Ke(g,e.artwork),o=Ge(g)};function u(){l||(l=e.events({onState(g){xt(g)&&(d=!0,n(g),m())},onStatus(g){a!==g&&(a=g,m())}}),e.state().then(g=>{d||!xt(g)||(n(g),m())},()=>{}))}function h(){l?.(),l=null}async function v(g){let y=await e.command(g);return y.signedOut&&a!=="signed-out"&&(a="signed-out",m()),y.ok&&xt(y.state)&&(!r||y.state.serial>r.serial)&&(n(y.state),m()),y}function _(g){return t.add(g),g(p()),()=>t.delete(g)}return{start:u,stop:h,command:v,subscribe:_,view:p}}var Ot=document.querySelector("chorus-app");if(Ot){Ot.mode=ge(window.location.search);let e=ye(Ht());Ot.store=e,e.start()}ve();
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
