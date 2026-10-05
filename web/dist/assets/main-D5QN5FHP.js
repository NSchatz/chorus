function Mt(r){let t=Math.min(1e3,Math.max(0,Math.round(Number(r)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function Lt(r,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(r)},"volume":${Mt(t)}}`}function zt(r,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(r)},"muted":${t?"true":"false"}}`}function ut(r,t){return`{"v":2,"t":"join","zone":${JSON.stringify(r)},"target":${JSON.stringify(t)}}`}function q(r){return`{"v":2,"t":"take","target":${JSON.stringify(r)}}`}function Nt(r,t){return`{"v":2,"t":"take","target":${JSON.stringify(r)},"source":${JSON.stringify(t)}}`}function Ut(r,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(r)},"volume":${Mt(t)}}`}function dt(r,t,e=""){let s=5381;for(let o of String(e))s=(Math.imul(s,33)^o.codePointAt(0))>>>0;return`${r}api/artwork?group=${encodeURIComponent(t)}${e?`#${s.toString(36)}`:""}`}function lt(r){return!!r&&(r.type==="opaqueredirect"||r.status===401)}var Rt="Signed out";async function Te(r){let t="";try{t=(await r.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail)return e.detail}catch{}return t||`the server answered ${r.status}`}var Pe={set:(r,t)=>globalThis.setTimeout(r,t),clear:r=>globalThis.clearTimeout(r)};function Ht({fetch:r=globalThis.fetch.bind(globalThis),base:t="../",timers:e=Pe}={}){async function s(){let d=await r(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store",redirect:"manual"});if(lt(d))throw Object.assign(new Error(Rt),{signedOut:!0});if(!d.ok)throw new Error(`the server answered ${d.status}`);return d.json()}async function o(d){let l;try{l=await r(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:d,redirect:"manual"})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(lt(l))return{ok:!1,refusal:Rt,signedOut:!0};if(!l.ok)return{ok:!1,refusal:await Te(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function i({onState:d,onStatus:l=()=>{}}){let p=!1,m=null,n=null,u=null,h=()=>{u!==null&&e.clear(u),u=null},v=()=>{h(),u=e.set(()=>m?.abort(),4e4)},_=y=>{let S=y.split(`
`).filter(U=>U.startsWith("data:")).map(U=>U.slice(5).replace(/^ /,"")).join(`
`);if(!S)return;let N;try{N=JSON.parse(S)}catch{return}l("live"),d(N)};async function g(){m=new AbortController,v();let y=!1;try{let S=await r(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",redirect:"manual",signal:m.signal});if(y=lt(S),!S.ok||!S.body)throw new Error(`the server answered ${S.status}`);let N=S.body.getReader();m.signal.addEventListener("abort",()=>N.cancel().catch(()=>{}));let U=new TextDecoder,H="";for(;;){let{done:Ee,value:Ce}=await N.read();if(Ee||p||m.signal.aborted)break;v(),H+=U.decode(Ce,{stream:!0}).replace(/\r\n?/g,`
`);let at;for(;(at=H.indexOf(`

`))!==-1;)_(H.slice(0,at)),H=H.slice(at+2)}}catch{}h(),!p&&(l(y?"signed-out":"lost"),n=e.set(()=>{n=null,g()},1e3))}return g(),()=>{p=!0,h(),n!==null&&e.clear(n),m?.abort()}}return{state:s,command:o,events:i,artwork:(d,l)=>dt(t,d,l)}}var G=globalThis,K=G.ShadowRoot&&(G.ShadyCSS===void 0||G.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,ct=Symbol(),It=new WeakMap,I=class{constructor(t,e,s){if(this._$cssResult$=!0,s!==ct)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(K&&t===void 0){let s=e!==void 0&&e.length===1;s&&(t=It.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),s&&It.set(e,t))}return t}toString(){return this.cssText}},Dt=r=>new I(typeof r=="string"?r:r+"",void 0,ct),b=(r,...t)=>{let e=r.length===1?r[0]:t.reduce((s,o,i)=>s+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(o)+r[i+1],r[0]);return new I(e,r,ct)},jt=(r,t)=>{if(K)r.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let s=document.createElement("style"),o=G.litNonce;o!==void 0&&s.setAttribute("nonce",o),s.textContent=e.cssText,r.appendChild(s)}},ht=K?r=>r:r=>r instanceof CSSStyleSheet?(t=>{let e="";for(let s of t.cssRules)e+=s.cssText;return Dt(e)})(r):r;var{is:Oe,defineProperty:Re,getOwnPropertyDescriptor:Me,getOwnPropertyNames:Le,getOwnPropertySymbols:ze,getPrototypeOf:Ne}=Object,Y=globalThis,Bt=Y.trustedTypes,Ue=Bt?Bt.emptyScript:"",He=Y.reactiveElementPolyfillSupport,D=(r,t)=>r,pt={toAttribute(r,t){switch(t){case Boolean:r=r?Ue:null;break;case Object:case Array:r=r==null?r:JSON.stringify(r)}return r},fromAttribute(r,t){let e=r;switch(t){case Boolean:e=r!==null;break;case Number:e=r===null?null:Number(r);break;case Object:case Array:try{e=JSON.parse(r)}catch{e=null}}return e}},Ft=(r,t)=>!Oe(r,t),Vt={attribute:!0,type:String,converter:pt,reflect:!1,useDefault:!1,hasChanged:Ft};Symbol.metadata??=Symbol("metadata"),Y.litPropertyMetadata??=new WeakMap;var w=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=Vt){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let s=Symbol(),o=this.getPropertyDescriptor(t,s,e);o!==void 0&&Re(this.prototype,t,o)}}static getPropertyDescriptor(t,e,s){let{get:o,set:i}=Me(this.prototype,t)??{get(){return this[e]},set(a){this[e]=a}};return{get:o,set(a){let d=o?.call(this);i?.call(this,a),this.requestUpdate(t,d,s)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??Vt}static _$Ei(){if(this.hasOwnProperty(D("elementProperties")))return;let t=Ne(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(D("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(D("properties"))){let e=this.properties,s=[...Le(e),...ze(e)];for(let o of s)this.createProperty(o,e[o])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[s,o]of e)this.elementProperties.set(s,o)}this._$Eh=new Map;for(let[e,s]of this.elementProperties){let o=this._$Eu(e,s);o!==void 0&&this._$Eh.set(o,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let s=new Set(t.flat(1/0).reverse());for(let o of s)e.unshift(ht(o))}else t!==void 0&&e.push(ht(t));return e}static _$Eu(t,e){let s=e.attribute;return s===!1?void 0:typeof s=="string"?s:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let s of e.keys())this.hasOwnProperty(s)&&(t.set(s,this[s]),delete this[s]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return jt(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,s){this._$AK(t,s)}_$ET(t,e){let s=this.constructor.elementProperties.get(t),o=this.constructor._$Eu(t,s);if(o!==void 0&&s.reflect===!0){let i=(s.converter?.toAttribute!==void 0?s.converter:pt).toAttribute(e,s.type);this._$Em=t,i==null?this.removeAttribute(o):this.setAttribute(o,i),this._$Em=null}}_$AK(t,e){let s=this.constructor,o=s._$Eh.get(t);if(o!==void 0&&this._$Em!==o){let i=s.getPropertyOptions(o),a=typeof i.converter=="function"?{fromAttribute:i.converter}:i.converter?.fromAttribute!==void 0?i.converter:pt;this._$Em=o;let d=a.fromAttribute(e,i.type);this[o]=d??this._$Ej?.get(o)??d,this._$Em=null}}requestUpdate(t,e,s,o=!1,i){if(t!==void 0){let a=this.constructor;if(o===!1&&(i=this[t]),s??=a.getPropertyOptions(t),!((s.hasChanged??Ft)(i,e)||s.useDefault&&s.reflect&&i===this._$Ej?.get(t)&&!this.hasAttribute(a._$Eu(t,s))))return;this.C(t,e,s)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:s,reflect:o,wrapped:i},a){s&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,a??e??this[t]),i!==!0||a!==void 0)||(this._$AL.has(t)||(this.hasUpdated||s||(e=void 0),this._$AL.set(t,e)),o===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[o,i]of this._$Ep)this[o]=i;this._$Ep=void 0}let s=this.constructor.elementProperties;if(s.size>0)for(let[o,i]of s){let{wrapped:a}=i,d=this[o];a!==!0||this._$AL.has(o)||d===void 0||this.C(o,void 0,i,d)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(s=>s.hostUpdate?.()),this.update(e)):this._$EM()}catch(s){throw t=!1,this._$EM(),s}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};w.elementStyles=[],w.shadowRootOptions={mode:"open"},w[D("elementProperties")]=new Map,w[D("finalized")]=new Map,He?.({ReactiveElement:w}),(Y.reactiveElementVersions??=[]).push("2.1.2");var ft=globalThis,Wt=r=>r,J=ft.trustedTypes,qt=J?J.createPolicy("lit-html",{createHTML:r=>r}):void 0,gt="$lit$",k=`lit$${Math.random().toFixed(9).slice(2)}$`,vt="?"+k,Ie=`<${vt}>`,T=document,B=()=>T.createComment(""),V=r=>r===null||typeof r!="object"&&typeof r!="function",$t=Array.isArray,Qt=r=>$t(r)||typeof r?.[Symbol.iterator]=="function",mt=`[ 	
\f\r]`,j=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Gt=/-->/g,Kt=/>/g,E=RegExp(`>|${mt}(?:([^\\s"'>=/]+)(${mt}*=${mt}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Yt=/'/g,Jt=/"/g,Zt=/^(?:script|style|textarea|title)$/i,_t=r=>(t,...e)=>({_$litType$:r,strings:t,values:e}),f=_t(1),cr=_t(2),hr=_t(3),A=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),Xt=new WeakMap,C=T.createTreeWalker(T,129);function te(r,t){if(!$t(r)||!r.hasOwnProperty("raw"))throw Error("invalid template strings array");return qt!==void 0?qt.createHTML(t):t}var ee=(r,t)=>{let e=r.length-1,s=[],o,i=t===2?"<svg>":t===3?"<math>":"",a=j;for(let d=0;d<e;d++){let l=r[d],p,m,n=-1,u=0;for(;u<l.length&&(a.lastIndex=u,m=a.exec(l),m!==null);)u=a.lastIndex,a===j?m[1]==="!--"?a=Gt:m[1]!==void 0?a=Kt:m[2]!==void 0?(Zt.test(m[2])&&(o=RegExp("</"+m[2],"g")),a=E):m[3]!==void 0&&(a=E):a===E?m[0]===">"?(a=o??j,n=-1):m[1]===void 0?n=-2:(n=a.lastIndex-m[2].length,p=m[1],a=m[3]===void 0?E:m[3]==='"'?Jt:Yt):a===Jt||a===Yt?a=E:a===Gt||a===Kt?a=j:(a=E,o=void 0);let h=a===E&&r[d+1].startsWith("/>")?" ":"";i+=a===j?l+Ie:n>=0?(s.push(p),l.slice(0,n)+gt+l.slice(n)+k+h):l+k+(n===-2?d:h)}return[te(r,i+(r[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),s]},F=class r{constructor({strings:t,_$litType$:e},s){let o;this.parts=[];let i=0,a=0,d=t.length-1,l=this.parts,[p,m]=ee(t,e);if(this.el=r.createElement(p,s),C.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(o=C.nextNode())!==null&&l.length<d;){if(o.nodeType===1){if(o.hasAttributes())for(let n of o.getAttributeNames())if(n.endsWith(gt)){let u=m[a++],h=o.getAttribute(n).split(k),v=/([.?@])?(.*)/.exec(u);l.push({type:1,index:i,name:v[2],strings:h,ctor:v[1]==="."?Q:v[1]==="?"?Z:v[1]==="@"?tt:O}),o.removeAttribute(n)}else n.startsWith(k)&&(l.push({type:6,index:i}),o.removeAttribute(n));if(Zt.test(o.tagName)){let n=o.textContent.split(k),u=n.length-1;if(u>0){o.textContent=J?J.emptyScript:"";for(let h=0;h<u;h++)o.append(n[h],B()),C.nextNode(),l.push({type:2,index:++i});o.append(n[u],B())}}}else if(o.nodeType===8)if(o.data===vt)l.push({type:2,index:i});else{let n=-1;for(;(n=o.data.indexOf(k,n+1))!==-1;)l.push({type:7,index:i}),n+=k.length-1}i++}}static createElement(t,e){let s=T.createElement("template");return s.innerHTML=t,s}};function P(r,t,e=r,s){if(t===A)return t;let o=s!==void 0?e._$Co?.[s]:e._$Cl,i=V(t)?void 0:t._$litDirective$;return o?.constructor!==i&&(o?._$AO?.(!1),i===void 0?o=void 0:(o=new i(r),o._$AT(r,e,s)),s!==void 0?(e._$Co??=[])[s]=o:e._$Cl=o),o!==void 0&&(t=P(r,o._$AS(r,t.values),o,s)),t}var X=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:s}=this._$AD,o=(t?.creationScope??T).importNode(e,!0);C.currentNode=o;let i=C.nextNode(),a=0,d=0,l=s[0];for(;l!==void 0;){if(a===l.index){let p;l.type===2?p=new R(i,i.nextSibling,this,t):l.type===1?p=new l.ctor(i,l.name,l.strings,this,t):l.type===6&&(p=new et(i,this,t)),this._$AV.push(p),l=s[++d]}a!==l?.index&&(i=C.nextNode(),a++)}return C.currentNode=T,o}p(t){let e=0;for(let s of this._$AV)s!==void 0&&(s.strings!==void 0?(s._$AI(t,s,e),e+=s.strings.length-2):s._$AI(t[e])),e++}},R=class r{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,s,o){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=s,this.options=o,this._$Cv=o?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=P(this,t,e),V(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==A&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):Qt(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&V(this._$AH)?this._$AA.nextSibling.data=t:this.T(T.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:s}=t,o=typeof s=="number"?this._$AC(t):(s.el===void 0&&(s.el=F.createElement(te(s.h,s.h[0]),this.options)),s);if(this._$AH?._$AD===o)this._$AH.p(e);else{let i=new X(o,this),a=i.u(this.options);i.p(e),this.T(a),this._$AH=i}}_$AC(t){let e=Xt.get(t.strings);return e===void 0&&Xt.set(t.strings,e=new F(t)),e}k(t){$t(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,s,o=0;for(let i of t)o===e.length?e.push(s=new r(this.O(B()),this.O(B()),this,this.options)):s=e[o],s._$AI(i),o++;o<e.length&&(this._$AR(s&&s._$AB.nextSibling,o),e.length=o)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let s=Wt(t).nextSibling;Wt(t).remove(),t=s}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},O=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,s,o,i){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=e,this._$AM=o,this.options=i,s.length>2||s[0]!==""||s[1]!==""?(this._$AH=Array(s.length-1).fill(new String),this.strings=s):this._$AH=c}_$AI(t,e=this,s,o){let i=this.strings,a=!1;if(i===void 0)t=P(this,t,e,0),a=!V(t)||t!==this._$AH&&t!==A,a&&(this._$AH=t);else{let d=t,l,p;for(t=i[0],l=0;l<i.length-1;l++)p=P(this,d[s+l],e,l),p===A&&(p=this._$AH[l]),a||=!V(p)||p!==this._$AH[l],p===c?t=c:t!==c&&(t+=(p??"")+i[l+1]),this._$AH[l]=p}a&&!o&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},Q=class extends O{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},Z=class extends O{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},tt=class extends O{constructor(t,e,s,o,i){super(t,e,s,o,i),this.type=5}_$AI(t,e=this){if((t=P(this,t,e,0)??c)===A)return;let s=this._$AH,o=t===c&&s!==c||t.capture!==s.capture||t.once!==s.once||t.passive!==s.passive,i=t!==c&&(s===c||o);o&&this.element.removeEventListener(this.name,this,s),i&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},et=class{constructor(t,e,s){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=s}get _$AU(){return this._$AM._$AU}_$AI(t){P(this,t)}},re={M:gt,P:k,A:vt,C:1,L:ee,R:X,D:Qt,V:P,I:R,H:O,N:Z,U:tt,B:Q,F:et},De=ft.litHtmlPolyfillSupport;De?.(F,R),(ft.litHtmlVersions??=[]).push("3.3.3");var se=(r,t,e)=>{let s=e?.renderBefore??t,o=s._$litPart$;if(o===void 0){let i=e?.renderBefore??null;s._$litPart$=o=new R(t.insertBefore(B(),i),i,void 0,e??{})}return o._$AI(r),o};var yt=globalThis,$=class extends w{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=se(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return A}};$._$litElement$=!0,$.finalized=!0,yt.litElementHydrateSupport?.({LitElement:$});var je=yt.litElementPolyfillSupport;je?.({LitElement:$});(yt.litElementVersions??=[]).push("4.2.2");function Be(r,t,e){let s=r.elementFromPoint?.(t,e)??null;for(;s?.shadowRoot?.elementFromPoint;){let o=s.shadowRoot.elementFromPoint(t,e);if(!o||o===s)break;s=o}return s}function Ve(r){for(let t=r;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var oe=(r,t,e)=>Ve(Be(r,t,e));function ie({root:r=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:s=()=>{}}={}){let o=null,i=()=>{let{handle:n,pointerId:u}=o;n.removeEventListener("pointermove",a),n.removeEventListener("pointerup",d),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),r.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(u)}catch{}o=null};function a(n){if(!(!o||n.pointerId!==o.pointerId)){if(!o.moving){if(Math.hypot(n.clientX-o.x,n.clientY-o.y)<8)return;o.moving=!0,t(o.room)}n.preventDefault(),e(oe(r,n.clientX,n.clientY))}}function d(n){if(!o||n.pointerId!==o.pointerId)return;let{room:u,moving:h}=o;if(i(),!h)return;let v=_=>{_.stopPropagation(),_.preventDefault()};r.addEventListener("click",v,!0),setTimeout(()=>r.removeEventListener("click",v,!0),0),s(u,oe(r,n.clientX,n.clientY))}function l(n){if(!o||n&&n.pointerId!==void 0&&n.pointerId!==o.pointerId)return;let{room:u,moving:h}=o;i(),h&&s(u,null)}function p(n){n.key==="Escape"&&l()}function m(n){if(o||n.isPrimary===!1||n.button>0)return;let u=n.composedPath().find(h=>h.dataset?.dragRoom);if(u){o={handle:u,room:u.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{u.setPointerCapture?.(n.pointerId)}catch{}u.addEventListener("pointermove",a),u.addEventListener("pointerup",d),u.addEventListener("pointercancel",l),u.addEventListener("lostpointercapture",l),r.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!o?.moving}}function W(r,t){return t.find(e=>e.id===r.group&&e.rooms.some(s=>s.id===r.id))??null}function ne(r,t,e){if(!r||!t)return null;let s=W(r,e);return t.kind==="alone"?s?q(r.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?s&&s.id===t.id?null:ut(r.id,t.id):t.kind==="room"?t.id===r.id||s&&s.rooms.some(o=>o.id===t.id)?null:ut(r.id,t.id):null}var bt=r=>r.kind==="alone"?"alone":`${r.kind}:${r.id}`;function ae(r){if(r==="alone")return{kind:"alone"};let t=String(r).indexOf(":");if(t<1)return null;let e=r.slice(0,t),s=r.slice(t+1);return(e==="room"||e==="group")&&s?{kind:e,id:s}:null}function le(r,t){let e=W(r,t);return e?bt({kind:"group",id:e.id}):"alone"}function ue(r,t,e){return[{value:"alone",label:"Alone"},...e.map(s=>({value:bt({kind:"group",id:s.id}),label:s.name})),...t.filter(s=>s.id!==r.id&&!W(s,e)).map(s=>({value:bt({kind:"room",id:s.id}),label:`With ${s.name}`}))]}var de={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},rt=r=>(...t)=>({_$litDirective$:r,values:t}),M=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,s){this._$Ct=t,this._$AM=e,this._$Ci=s}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:Fe}=re,ce=r=>r;var he=()=>document.createComment(""),L=(r,t,e)=>{let s=r._$AA.parentNode,o=t===void 0?r._$AB:t._$AA;if(e===void 0){let i=s.insertBefore(he(),o),a=s.insertBefore(he(),o);e=new Fe(i,a,r,r.options)}else{let i=e._$AB.nextSibling,a=e._$AM,d=a!==r;if(d){let l;e._$AQ?.(r),e._$AM=r,e._$AP!==void 0&&(l=r._$AU)!==a._$AU&&e._$AP(l)}if(i!==o||d){let l=e._$AA;for(;l!==i;){let p=ce(l).nextSibling;ce(s).insertBefore(l,o),l=p}}}return e},x=(r,t,e=r)=>(r._$AI(t,e),r),We={},st=(r,t=We)=>r._$AH=t,pe=r=>r._$AH,ot=r=>{r._$AR(),r._$AA.remove()};var me=(r,t,e)=>{let s=new Map;for(let o=t;o<=e;o++)s.set(r[o],o);return s},it=rt(class extends M{constructor(r){if(super(r),r.type!==de.CHILD)throw Error("repeat() can only be used in text expressions")}dt(r,t,e){let s;e===void 0?e=t:t!==void 0&&(s=t);let o=[],i=[],a=0;for(let d of r)o[a]=s?s(d,a):a,i[a]=e(d,a),a++;return{values:i,keys:o}}render(r,t,e){return this.dt(r,t,e).values}update(r,[t,e,s]){let o=pe(r),{values:i,keys:a}=this.dt(t,e,s);if(!Array.isArray(o))return this.ut=a,i;let d=this.ut??=[],l=[],p,m,n=0,u=o.length-1,h=0,v=i.length-1;for(;n<=u&&h<=v;)if(o[n]===null)n++;else if(o[u]===null)u--;else if(d[n]===a[h])l[h]=x(o[n],i[h]),n++,h++;else if(d[u]===a[v])l[v]=x(o[u],i[v]),u--,v--;else if(d[n]===a[v])l[v]=x(o[n],i[v]),L(r,l[v+1],o[n]),n++,v--;else if(d[u]===a[h])l[h]=x(o[u],i[h]),L(r,o[n],o[u]),u--,h++;else if(p===void 0&&(p=me(a,h,v),m=me(d,n,u)),p.has(d[n]))if(p.has(d[u])){let _=m.get(a[h]),g=_!==void 0?o[_]:null;if(g===null){let y=L(r,o[n]);x(y,i[h]),l[h]=y}else l[h]=x(g,i[h]),L(r,o[n],g),o[_]=null;h++}else ot(o[u]),u--;else ot(o[n]),n++;for(;h<=v;){let _=L(r,l[v+1]);x(_,i[h]),l[h++]=_}for(;n<=u;){let _=o[n++];_!==null&&ot(_)}return this.ut=a,st(r,l),A}});var fe=rt(class extends M{constructor(){super(...arguments),this.key=c}render(r,t){return this.key=r,t}update(r,[t,e]){return t!==this.key&&(st(r),this.key=t),e}});var qe={playing:"Playing",paused:"Paused",buffering:"Buffering"};function Ge(r,t=[]){if(!r)return"Unavailable";let e=t.find(a=>a.source===r);if(e)return e.label;if(r==="stream")return"The server's stream";if(r==="none")return"Nothing";let[s,...o]=r.split(":"),i=o.join(":");return s==="line-in"&&i?`Input ${i}`:s==="player"&&i?`Network player ${i}`:s==="chime"&&i?`Chime ${i}`:s==="soloist"&&i?"Spotify":r}var wt=class extends ${static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:Nt(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=f`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:fe(t.artwork,f`<img
        data-artwork="image"
        src=${t.artwork}
        alt="Artwork for ${this.name}"
        @error=${this._onArtworkError}
      />`)}render(){let t=this.nowPlaying,e=this.inputs??[];return f`
      ${t?f`<div class="now" data-now-playing=${t.state??"unknown"}>
            ${this._artwork(t)}
            <div class="words">
              <p data-title>${t.title??"Unknown title"}</p>
              ${t.artist?f`<p data-artist>${t.artist}</p>`:c}
              ${t.album?f`<p data-album>${t.album}</p>`:c}
              <p data-state>${qe[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${Ge(this.source,e)}</p>
      ${this.pick&&e.length>0?f`<ul aria-label="Inputs for ${this.name}">
            ${e.map(s=>f`<li data-input=${s.id}>
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
    `}};customElements.define("chorus-playing",wt);var Ke=r=>`${Math.round(r/10)}%`,kt=class extends ${static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let s=t.has("refusal")&&!!this.refusal;s&&(this._dragged=null),(!this._sliderHeld||s)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Ut(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(q(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(i=>i.id)),s=t.defined??[],o=new Set(s.map(i=>i.id));return[...s.map(i=>({...i,playing:e.has(i.id)})),...t.rooms.filter(i=>!o.has(i.id)).map(i=>({...i,playing:!0}))]}render(){let t=this.group;if(!t)return c;let e=t.volume===null?"":Ke(this._dragged??t.volume);return f`
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
              aria-valuetext=${e}
              @focus=${this._onSliderFocus}
              @blur=${this._onSliderBlur}
              @input=${this._onSliderInput}
              @change=${this._onSliderChange}
            />
            <span class="figure" data-volume>${e}</span>
          </div>`}
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-group-card",kt);var At=class extends ${static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
  `;constructor(){super(),this.groups=null,this.inputs=[],this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return f`
      ${this.groups!==null&&t.length===0?f`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:c}
      <ul>
        ${it(t,s=>s.id,s=>f`<li
              data-group=${s.id}
              data-drop="group"
              data-drop-id=${s.id}
              ?data-over=${e?.kind==="group"&&e.id===s.id}
            >
              <chorus-group-card
                .group=${s}
                .inputs=${this.inputs}
                .refusal=${this.refusals[s.id]??""}
              ></chorus-group-card>
            </li>`)}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${e?.kind==="alone"}>
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:c}
      </p>
    `}};customElements.define("chorus-groups",At);var ge=Object.freeze(["phone","desktop"]),Ye=48,Je=`(min-width: ${Ye}em)`;function ve(r,t=globalThis){if(typeof t?.matchMedia!="function")return r("phone"),()=>{};let e=t.matchMedia(Je),s=()=>r(e.matches?"desktop":"phone");return e.addEventListener("change",s),s(),()=>e.removeEventListener("change",s)}var _e=Object.freeze(["app","kiosk"]),St="chorus.kiosk",$e="1";function Xe(r){let t=new URLSearchParams(r).get("kiosk");return t===null?null:t==="0"||t==="false"?"app":"kiosk"}function ye(r,t){let e=Xe(r);try{if(e==="kiosk")t?.setItem(St,$e);else if(e==="app")t?.removeItem(St);else return t?.getItem(St)===$e?"kiosk":"app"}catch{}return e??"app"}function be(r=globalThis){try{return r.localStorage??null}catch{return null}}var Qe={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Ze=r=>`${Math.round(r/10)}%`,xt=class extends ${static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let s=this._slider;if(!s||this.room.volume===null)return;let o=t.has("refusal")&&!!this.refusal;o&&(this._dragged=null),(!this._sliderHeld||o)&&(s.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Lt(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let s=ae(e);s&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:s},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(zt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let e=t.volume===null?"Unavailable":Ze(this._dragged??t.volume);return f`
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
                    ${Qe[s.role]??s.role}: ${s.name}
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
          ${this.places.map(s=>f`<option value=${s.value} ?selected=${s.value===this.place}>${s.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:c}</p>
    `}};customElements.define("chorus-room-card",xt);var Et=class extends ${static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="signed-out"?this.rooms===null?"":"This is the last known state.":this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],s=this.over;return f`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?f`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${it(t??[],o=>o.id,o=>f`<li
              data-room=${o.id}
              data-drop="room"
              data-drop-id=${o.id}
              ?data-moving=${this.moving?.id===o.id}
              ?data-over=${s?.kind==="room"&&s.id===o.id&&this.moving?.id!==o.id}
            >
              <chorus-room-card
                .room=${o}
                .inputs=${this.inputs}
                .refusal=${this.refusals[o.id]??""}
                .places=${ue(o,t,e)}
                .place=${le(o,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",Et);var Ct=class extends ${static properties={mode:{type:String,reflect:!0},layout:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.mode="app",this.layout="phone",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._moving=null,this._over=null,this._unsubscribe=null,this._unwatch=null,this._drag=ie({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!W(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){_e.includes(this.mode)||(this.mode="app"),ge.includes(this.layout)||(this.layout="phone"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow(),this._unwatch?.(),this._unwatch=ve(t=>{this.layout=t})}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._unwatch?.(),this._unwatch=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""};let s=await this.store.command(e);s.ok||(this._refusals={...this._refusals,[t]:s.refusal})}_onCommand(t){let{subject:e,room:s,body:o}=t.detail;this._send(e??s,o)}_move(t,e){let s=this._room(t),o=ne(s,e,this._groups);o&&this._send(t,o)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}_onGo(t){let e=this.renderRoot.querySelector(t.currentTarget.dataset.go==="rooms"?"main":"section");e&&(e.scrollIntoView?.({block:"start"}),e.focus?.({preventScroll:!0}))}_signedOut(){return this._view.status!=="signed-out"?c:f`
      <p role="alert" data-signed-out>
        Signed out. <a href=${globalThis.location?.href??"./"} aria-label="Sign in">Sign in</a> to go on.
      </p>
    `}render(){return f`
      <header>
        <h1>chorus</h1>
        <nav aria-label="Sections">
          <button type="button" data-go="groups" aria-label="Go to groups" @click=${this._onGo}>Groups</button>
          <button type="button" data-go="rooms" aria-label="Go to rooms" @click=${this._onGo}>Rooms</button>
        </nav>
      </header>
      ${this._signedOut()}
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
    `}};customElements.define("chorus-app",Ct);var tr="sw.js";async function we(r=globalThis.navigator){let t=r?.serviceWorker;if(!t||typeof t.register!="function")return null;try{return await t.register(tr,{scope:"./",updateViaCache:"none"})}catch{return null}}var er=(r,t)=>dt("../",r,t),z=r=>typeof r=="string"&&r?r:null,rr=["playing","paused","buffering"];function ke(r,t=er){let e=r&&Array.isArray(r.groups)?r.groups:[],s=new Map;for(let o of e){if(!o||typeof o!="object"||typeof o.id!="string"||!o.id)continue;let i=o.now_playing&&typeof o.now_playing=="object"?o.now_playing:null,a=i?z(i.art_url):null;s.set(o.id,{source:z(o.source),nowPlaying:i&&{title:z(i.title),artist:z(i.artist),album:z(i.album),state:rr.includes(i.state)?i.state:null,via:z(i.via),artwork:a?t(o.id,a):null}})}return s}var Pt={source:null,nowPlaying:null};function sr(r){let t=r&&Array.isArray(r.inputs)?r.inputs:[],e=new Map((r&&Array.isArray(r.input_labels)?r.input_labels:[]).filter(s=>s&&typeof s.input=="string"&&typeof s.name=="string"&&s.name).map(s=>[s.input,s.name]));return t.filter(s=>typeof s=="string"&&s).map(s=>({id:s,source:`line-in:${s}`,label:e.get(s)??s}))}function or(r,t,e){if(!r||typeof r!="object"||typeof r.id!="string"||!r.id)return null;let s=Array.isArray(r.bond)?r.bond:[],o=typeof r.group=="string"&&r.group?r.group:r.id;return{id:r.id,name:typeof r.name=="string"&&r.name?r.name:r.id,volume:Ot(r.volume),muted:typeof r.muted=="boolean"?r.muted:null,group:o,...o===r.id&&e.get(o)||Pt,bond:s.filter(i=>i&&typeof i.endpoint=="string"&&typeof i.role=="string").map(i=>({endpoint:i.endpoint,name:t.get(i.endpoint)??i.endpoint,role:i.role}))}}function Ae(r,t){let e=r&&Array.isArray(r.zones)?r.zones:[],s=r&&Array.isArray(r.speakers)?r.speakers:[],o=new Map(s.filter(a=>a&&typeof a.id=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.id,a.name])),i=ke(r,t);return e.map(a=>or(a,o,i)).filter(Boolean)}var Ot=r=>typeof r=="number"&&r>=0&&r<=1?Math.round(r*1e3):null;function ir(r,t){let e=new Map(Ae(r,t).map(n=>[n.id,n.name])),s=ke(r,t),o=n=>({id:n,name:e.get(n)??n}),i=n=>Array.isArray(n)?n:[],a=n=>i(n).filter(u=>typeof u=="string"&&u).map(o),d=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=i(r?.groups).filter(d),p=i(r?.saved_groups).filter(d),m=new Set(p.map(n=>n.id));return[...p.map(n=>{let u=l.find(h=>h.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:a(n.zones),rooms:u?a(u.zones):[],volume:u?Ot(u.volume):null,...u&&s.get(n.id)||Pt}}),...l.filter(n=>n.kind==="live"&&!m.has(n.id)).map(n=>{let u=a(n.zones);return{id:n.id,name:u.map(h=>h.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:u,volume:Ot(n.volume),...s.get(n.id)??Pt}})]}var Tt=r=>!!r&&typeof r=="object"&&Array.isArray(r.zones);function Se(r){let t=new Set,e=null,s=[],o=[],i=[],a="connecting",d=!1,l=null,p=()=>({state:e,rooms:s,groups:o,inputs:i,status:a}),m=()=>{let g=p();for(let y of[...t])y(g)},n=g=>{e=g,s=Ae(g,r.artwork),o=ir(g,r.artwork),i=sr(g)};function u(){l||(l=r.events({onState(g){Tt(g)&&(d=!0,n(g),m())},onStatus(g){a!==g&&(a=g,m())}}),r.state().then(g=>{d||!Tt(g)||(n(g),m())},()=>{}))}function h(){l?.(),l=null}async function v(g){let y=await r.command(g);return y.signedOut&&a!=="signed-out"&&(a="signed-out",m()),y.ok&&Tt(y.state)&&(!e||y.state.serial>e.serial)&&(n(y.state),m()),y}function _(g){return t.add(g),g(p()),()=>t.delete(g)}return{start:u,stop:h,command:v,subscribe:_,view:p}}function xe({navigator:r=globalThis.navigator,document:t=globalThis.document}={}){let e=null;try{e=r?.wakeLock??null}catch{e=null}if(!e||typeof e.request!="function"||typeof t?.addEventListener!="function")return{supported:!1,held:()=>!1,settled:async()=>{},stop:async()=>{}};let s=null,o=null,i=!1,a=async l=>{try{await l.release()}catch{}},d=()=>{i||s||o||t.visibilityState!=="visible"||(o=(async()=>{try{let l=await e.request("screen");if(i){await a(l);return}s=l,l.addEventListener?.("release",()=>{s===l&&(s=null)})}catch{}finally{o=null}})())};return t.addEventListener("visibilitychange",d),d(),{supported:!0,held:()=>s!==null&&s.released!==!0,settled:async()=>{for(;o;)await o},stop:async()=>{for(i=!0,t.removeEventListener("visibilitychange",d);o;)await o;let l=s;s=null,l&&await a(l)}}}var nt=document.querySelector("chorus-app");if(nt){nt.mode=ye(window.location.search,be(window)),nt.mode==="kiosk"&&xe();let r=Se(Ht());nt.store=r,r.start()}we();
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
