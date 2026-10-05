function Pt(r){let t=Math.min(1e3,Math.max(0,Math.round(Number(r)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function Tt(r,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(r)},"volume":${Pt(t)}}`}function Mt(r,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(r)},"muted":${t?"true":"false"}}`}function nt(r,t){return`{"v":2,"t":"join","zone":${JSON.stringify(r)},"target":${JSON.stringify(t)}}`}function q(r){return`{"v":2,"t":"take","target":${JSON.stringify(r)}}`}function Rt(r,t){return`{"v":2,"t":"take","target":${JSON.stringify(r)},"source":${JSON.stringify(t)}}`}function Ot(r,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(r)},"volume":${Pt(t)}}`}function at(r,t,e=""){let s=5381;for(let i of String(e))s=(Math.imul(s,33)^i.codePointAt(0))>>>0;return`${r}api/artwork?group=${encodeURIComponent(t)}${e?`#${s.toString(36)}`:""}`}async function _e(r){let t="";try{t=(await r.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail)return e.detail}catch{}return t||`the server answered ${r.status}`}var ye={set:(r,t)=>globalThis.setTimeout(r,t),clear:r=>globalThis.clearTimeout(r)};function Nt({fetch:r=globalThis.fetch.bind(globalThis),base:t="../",timers:e=ye}={}){async function s(){let d=await r(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store"});if(!d.ok)throw new Error(`the server answered ${d.status}`);return d.json()}async function i(d){let l;try{l=await r(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:d})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(!l.ok)return{ok:!1,refusal:await _e(l)};try{return{ok:!0,state:await l.json()}}catch{return{ok:!0,state:null}}}function o({onState:d,onStatus:l=()=>{}}){let p=!1,m=null,n=null,u=null,h=()=>{u!==null&&e.clear(u),u=null},v=()=>{h(),u=e.set(()=>m?.abort(),4e4)},y=_=>{let U=_.split(`
`).filter(w=>w.startsWith("data:")).map(w=>w.slice(5).replace(/^ /,"")).join(`
`);if(!U)return;let F;try{F=JSON.parse(U)}catch{return}l("live"),d(F)};async function g(){m=new AbortController,v();try{let _=await r(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",signal:m.signal});if(!_.ok||!_.body)throw new Error(`the server answered ${_.status}`);let U=_.body.getReader();m.signal.addEventListener("abort",()=>U.cancel().catch(()=>{}));let F=new TextDecoder,w="";for(;;){let{done:ve,value:$e}=await U.read();if(ve||p||m.signal.aborted)break;v(),w+=F.decode($e,{stream:!0}).replace(/\r\n?/g,`
`);let ot;for(;(ot=w.indexOf(`

`))!==-1;)y(w.slice(0,ot)),w=w.slice(ot+2)}}catch{}h(),!p&&(l("lost"),n=e.set(()=>{n=null,g()},1e3))}return g(),()=>{p=!0,h(),n!==null&&e.clear(n),m?.abort()}}return{state:s,command:i,events:o,artwork:(d,l)=>at(t,d,l)}}var J=globalThis,W=J.ShadowRoot&&(J.ShadyCSS===void 0||J.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,lt=Symbol(),Lt=new WeakMap,H=class{constructor(t,e,s){if(this._$cssResult$=!0,s!==lt)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(W&&t===void 0){let s=e!==void 0&&e.length===1;s&&(t=Lt.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),s&&Lt.set(e,t))}return t}toString(){return this.cssText}},Ut=r=>new H(typeof r=="string"?r:r+"",void 0,lt),b=(r,...t)=>{let e=r.length===1?r[0]:t.reduce((s,i,o)=>s+(a=>{if(a._$cssResult$===!0)return a.cssText;if(typeof a=="number")return a;throw Error("Value passed to 'css' function must be a 'css' function result: "+a+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(i)+r[o+1],r[0]);return new H(e,r,lt)},Ht=(r,t)=>{if(W)r.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let s=document.createElement("style"),i=J.litNonce;i!==void 0&&s.setAttribute("nonce",i),s.textContent=e.cssText,r.appendChild(s)}},ut=W?r=>r:r=>r instanceof CSSStyleSheet?(t=>{let e="";for(let s of t.cssRules)e+=s.cssText;return Ut(e)})(r):r;var{is:be,defineProperty:we,getOwnPropertyDescriptor:Ae,getOwnPropertyNames:Se,getOwnPropertySymbols:ke,getPrototypeOf:Ee}=Object,Y=globalThis,zt=Y.trustedTypes,xe=zt?zt.emptyScript:"",Ce=Y.reactiveElementPolyfillSupport,z=(r,t)=>r,dt={toAttribute(r,t){switch(t){case Boolean:r=r?xe:null;break;case Object:case Array:r=r==null?r:JSON.stringify(r)}return r},fromAttribute(r,t){let e=r;switch(t){case Boolean:e=r!==null;break;case Number:e=r===null?null:Number(r);break;case Object:case Array:try{e=JSON.parse(r)}catch{e=null}}return e}},It=(r,t)=>!be(r,t),Dt={attribute:!0,type:String,converter:dt,reflect:!1,useDefault:!1,hasChanged:It};Symbol.metadata??=Symbol("metadata"),Y.litPropertyMetadata??=new WeakMap;var A=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=Dt){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let s=Symbol(),i=this.getPropertyDescriptor(t,s,e);i!==void 0&&we(this.prototype,t,i)}}static getPropertyDescriptor(t,e,s){let{get:i,set:o}=Ae(this.prototype,t)??{get(){return this[e]},set(a){this[e]=a}};return{get:i,set(a){let d=i?.call(this);o?.call(this,a),this.requestUpdate(t,d,s)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??Dt}static _$Ei(){if(this.hasOwnProperty(z("elementProperties")))return;let t=Ee(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(z("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(z("properties"))){let e=this.properties,s=[...Se(e),...ke(e)];for(let i of s)this.createProperty(i,e[i])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[s,i]of e)this.elementProperties.set(s,i)}this._$Eh=new Map;for(let[e,s]of this.elementProperties){let i=this._$Eu(e,s);i!==void 0&&this._$Eh.set(i,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let s=new Set(t.flat(1/0).reverse());for(let i of s)e.unshift(ut(i))}else t!==void 0&&e.push(ut(t));return e}static _$Eu(t,e){let s=e.attribute;return s===!1?void 0:typeof s=="string"?s:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let s of e.keys())this.hasOwnProperty(s)&&(t.set(s,this[s]),delete this[s]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Ht(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,s){this._$AK(t,s)}_$ET(t,e){let s=this.constructor.elementProperties.get(t),i=this.constructor._$Eu(t,s);if(i!==void 0&&s.reflect===!0){let o=(s.converter?.toAttribute!==void 0?s.converter:dt).toAttribute(e,s.type);this._$Em=t,o==null?this.removeAttribute(i):this.setAttribute(i,o),this._$Em=null}}_$AK(t,e){let s=this.constructor,i=s._$Eh.get(t);if(i!==void 0&&this._$Em!==i){let o=s.getPropertyOptions(i),a=typeof o.converter=="function"?{fromAttribute:o.converter}:o.converter?.fromAttribute!==void 0?o.converter:dt;this._$Em=i;let d=a.fromAttribute(e,o.type);this[i]=d??this._$Ej?.get(i)??d,this._$Em=null}}requestUpdate(t,e,s,i=!1,o){if(t!==void 0){let a=this.constructor;if(i===!1&&(o=this[t]),s??=a.getPropertyOptions(t),!((s.hasChanged??It)(o,e)||s.useDefault&&s.reflect&&o===this._$Ej?.get(t)&&!this.hasAttribute(a._$Eu(t,s))))return;this.C(t,e,s)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:s,reflect:i,wrapped:o},a){s&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,a??e??this[t]),o!==!0||a!==void 0)||(this._$AL.has(t)||(this.hasUpdated||s||(e=void 0),this._$AL.set(t,e)),i===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[i,o]of this._$Ep)this[i]=o;this._$Ep=void 0}let s=this.constructor.elementProperties;if(s.size>0)for(let[i,o]of s){let{wrapped:a}=o,d=this[i];a!==!0||this._$AL.has(i)||d===void 0||this.C(i,void 0,o,d)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(s=>s.hostUpdate?.()),this.update(e)):this._$EM()}catch(s){throw t=!1,this._$EM(),s}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};A.elementStyles=[],A.shadowRootOptions={mode:"open"},A[z("elementProperties")]=new Map,A[z("finalized")]=new Map,Ce?.({ReactiveElement:A}),(Y.reactiveElementVersions??=[]).push("2.1.2");var ht=globalThis,jt=r=>r,G=ht.trustedTypes,Bt=G?G.createPolicy("lit-html",{createHTML:r=>r}):void 0,pt="$lit$",S=`lit$${Math.random().toFixed(9).slice(2)}$`,mt="?"+S,Pe=`<${mt}>`,P=document,I=()=>P.createComment(""),j=r=>r===null||typeof r!="object"&&typeof r!="function",ft=Array.isArray,Yt=r=>ft(r)||typeof r?.[Symbol.iterator]=="function",ct=`[ 	
\f\r]`,D=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Vt=/-->/g,Ft=/>/g,x=RegExp(`>|${ct}(?:([^\\s"'>=/]+)(${ct}*=${ct}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),qt=/'/g,Jt=/"/g,Gt=/^(?:script|style|textarea|title)$/i,gt=r=>(t,...e)=>({_$litType$:r,strings:t,values:e}),f=gt(1),Xe=gt(2),Qe=gt(3),k=Symbol.for("lit-noChange"),c=Symbol.for("lit-nothing"),Wt=new WeakMap,C=P.createTreeWalker(P,129);function Kt(r,t){if(!ft(r)||!r.hasOwnProperty("raw"))throw Error("invalid template strings array");return Bt!==void 0?Bt.createHTML(t):t}var Xt=(r,t)=>{let e=r.length-1,s=[],i,o=t===2?"<svg>":t===3?"<math>":"",a=D;for(let d=0;d<e;d++){let l=r[d],p,m,n=-1,u=0;for(;u<l.length&&(a.lastIndex=u,m=a.exec(l),m!==null);)u=a.lastIndex,a===D?m[1]==="!--"?a=Vt:m[1]!==void 0?a=Ft:m[2]!==void 0?(Gt.test(m[2])&&(i=RegExp("</"+m[2],"g")),a=x):m[3]!==void 0&&(a=x):a===x?m[0]===">"?(a=i??D,n=-1):m[1]===void 0?n=-2:(n=a.lastIndex-m[2].length,p=m[1],a=m[3]===void 0?x:m[3]==='"'?Jt:qt):a===Jt||a===qt?a=x:a===Vt||a===Ft?a=D:(a=x,i=void 0);let h=a===x&&r[d+1].startsWith("/>")?" ":"";o+=a===D?l+Pe:n>=0?(s.push(p),l.slice(0,n)+pt+l.slice(n)+S+h):l+S+(n===-2?d:h)}return[Kt(r,o+(r[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),s]},B=class r{constructor({strings:t,_$litType$:e},s){let i;this.parts=[];let o=0,a=0,d=t.length-1,l=this.parts,[p,m]=Xt(t,e);if(this.el=r.createElement(p,s),C.currentNode=this.el.content,e===2||e===3){let n=this.el.content.firstChild;n.replaceWith(...n.childNodes)}for(;(i=C.nextNode())!==null&&l.length<d;){if(i.nodeType===1){if(i.hasAttributes())for(let n of i.getAttributeNames())if(n.endsWith(pt)){let u=m[a++],h=i.getAttribute(n).split(S),v=/([.?@])?(.*)/.exec(u);l.push({type:1,index:o,name:v[2],strings:h,ctor:v[1]==="."?X:v[1]==="?"?Q:v[1]==="@"?Z:M}),i.removeAttribute(n)}else n.startsWith(S)&&(l.push({type:6,index:o}),i.removeAttribute(n));if(Gt.test(i.tagName)){let n=i.textContent.split(S),u=n.length-1;if(u>0){i.textContent=G?G.emptyScript:"";for(let h=0;h<u;h++)i.append(n[h],I()),C.nextNode(),l.push({type:2,index:++o});i.append(n[u],I())}}}else if(i.nodeType===8)if(i.data===mt)l.push({type:2,index:o});else{let n=-1;for(;(n=i.data.indexOf(S,n+1))!==-1;)l.push({type:7,index:o}),n+=S.length-1}o++}}static createElement(t,e){let s=P.createElement("template");return s.innerHTML=t,s}};function T(r,t,e=r,s){if(t===k)return t;let i=s!==void 0?e._$Co?.[s]:e._$Cl,o=j(t)?void 0:t._$litDirective$;return i?.constructor!==o&&(i?._$AO?.(!1),o===void 0?i=void 0:(i=new o(r),i._$AT(r,e,s)),s!==void 0?(e._$Co??=[])[s]=i:e._$Cl=i),i!==void 0&&(t=T(r,i._$AS(r,t.values),i,s)),t}var K=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:s}=this._$AD,i=(t?.creationScope??P).importNode(e,!0);C.currentNode=i;let o=C.nextNode(),a=0,d=0,l=s[0];for(;l!==void 0;){if(a===l.index){let p;l.type===2?p=new R(o,o.nextSibling,this,t):l.type===1?p=new l.ctor(o,l.name,l.strings,this,t):l.type===6&&(p=new tt(o,this,t)),this._$AV.push(p),l=s[++d]}a!==l?.index&&(o=C.nextNode(),a++)}return C.currentNode=P,i}p(t){let e=0;for(let s of this._$AV)s!==void 0&&(s.strings!==void 0?(s._$AI(t,s,e),e+=s.strings.length-2):s._$AI(t[e])),e++}},R=class r{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,s,i){this.type=2,this._$AH=c,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=s,this.options=i,this._$Cv=i?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=T(this,t,e),j(t)?t===c||t==null||t===""?(this._$AH!==c&&this._$AR(),this._$AH=c):t!==this._$AH&&t!==k&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):Yt(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==c&&j(this._$AH)?this._$AA.nextSibling.data=t:this.T(P.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:s}=t,i=typeof s=="number"?this._$AC(t):(s.el===void 0&&(s.el=B.createElement(Kt(s.h,s.h[0]),this.options)),s);if(this._$AH?._$AD===i)this._$AH.p(e);else{let o=new K(i,this),a=o.u(this.options);o.p(e),this.T(a),this._$AH=o}}_$AC(t){let e=Wt.get(t.strings);return e===void 0&&Wt.set(t.strings,e=new B(t)),e}k(t){ft(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,s,i=0;for(let o of t)i===e.length?e.push(s=new r(this.O(I()),this.O(I()),this,this.options)):s=e[i],s._$AI(o),i++;i<e.length&&(this._$AR(s&&s._$AB.nextSibling,i),e.length=i)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let s=jt(t).nextSibling;jt(t).remove(),t=s}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},M=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,s,i,o){this.type=1,this._$AH=c,this._$AN=void 0,this.element=t,this.name=e,this._$AM=i,this.options=o,s.length>2||s[0]!==""||s[1]!==""?(this._$AH=Array(s.length-1).fill(new String),this.strings=s):this._$AH=c}_$AI(t,e=this,s,i){let o=this.strings,a=!1;if(o===void 0)t=T(this,t,e,0),a=!j(t)||t!==this._$AH&&t!==k,a&&(this._$AH=t);else{let d=t,l,p;for(t=o[0],l=0;l<o.length-1;l++)p=T(this,d[s+l],e,l),p===k&&(p=this._$AH[l]),a||=!j(p)||p!==this._$AH[l],p===c?t=c:t!==c&&(t+=(p??"")+o[l+1]),this._$AH[l]=p}a&&!i&&this.j(t)}j(t){t===c?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},X=class extends M{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===c?void 0:t}},Q=class extends M{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==c)}},Z=class extends M{constructor(t,e,s,i,o){super(t,e,s,i,o),this.type=5}_$AI(t,e=this){if((t=T(this,t,e,0)??c)===k)return;let s=this._$AH,i=t===c&&s!==c||t.capture!==s.capture||t.once!==s.once||t.passive!==s.passive,o=t!==c&&(s===c||i);i&&this.element.removeEventListener(this.name,this,s),o&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},tt=class{constructor(t,e,s){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=s}get _$AU(){return this._$AM._$AU}_$AI(t){T(this,t)}},Qt={M:pt,P:S,A:mt,C:1,L:Xt,R:K,D:Yt,V:T,I:R,H:M,N:Q,U:Z,B:X,F:tt},Te=ht.litHtmlPolyfillSupport;Te?.(B,R),(ht.litHtmlVersions??=[]).push("3.3.3");var Zt=(r,t,e)=>{let s=e?.renderBefore??t,i=s._$litPart$;if(i===void 0){let o=e?.renderBefore??null;s._$litPart$=i=new R(t.insertBefore(I(),o),o,void 0,e??{})}return i._$AI(r),i};var vt=globalThis,$=class extends A{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=Zt(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return k}};$._$litElement$=!0,$.finalized=!0,vt.litElementHydrateSupport?.({LitElement:$});var Me=vt.litElementPolyfillSupport;Me?.({LitElement:$});(vt.litElementVersions??=[]).push("4.2.2");function Re(r,t,e){let s=r.elementFromPoint?.(t,e)??null;for(;s?.shadowRoot?.elementFromPoint;){let i=s.shadowRoot.elementFromPoint(t,e);if(!i||i===s)break;s=i}return s}function Oe(r){for(let t=r;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var te=(r,t,e)=>Oe(Re(r,t,e));function ee({root:r=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:s=()=>{}}={}){let i=null,o=()=>{let{handle:n,pointerId:u}=i;n.removeEventListener("pointermove",a),n.removeEventListener("pointerup",d),n.removeEventListener("pointercancel",l),n.removeEventListener("lostpointercapture",l),r.removeEventListener("keydown",p,!0);try{n.releasePointerCapture?.(u)}catch{}i=null};function a(n){if(!(!i||n.pointerId!==i.pointerId)){if(!i.moving){if(Math.hypot(n.clientX-i.x,n.clientY-i.y)<8)return;i.moving=!0,t(i.room)}n.preventDefault(),e(te(r,n.clientX,n.clientY))}}function d(n){if(!i||n.pointerId!==i.pointerId)return;let{room:u,moving:h}=i;if(o(),!h)return;let v=y=>{y.stopPropagation(),y.preventDefault()};r.addEventListener("click",v,!0),setTimeout(()=>r.removeEventListener("click",v,!0),0),s(u,te(r,n.clientX,n.clientY))}function l(n){if(!i||n&&n.pointerId!==void 0&&n.pointerId!==i.pointerId)return;let{room:u,moving:h}=i;o(),h&&s(u,null)}function p(n){n.key==="Escape"&&l()}function m(n){if(i||n.isPrimary===!1||n.button>0)return;let u=n.composedPath().find(h=>h.dataset?.dragRoom);if(u){i={handle:u,room:u.dataset.dragRoom,pointerId:n.pointerId,x:n.clientX,y:n.clientY,moving:!1};try{u.setPointerCapture?.(n.pointerId)}catch{}u.addEventListener("pointermove",a),u.addEventListener("pointerup",d),u.addEventListener("pointercancel",l),u.addEventListener("lostpointercapture",l),r.addEventListener("keydown",p,!0)}}return{begin:m,cancel:()=>l(),active:()=>!!i?.moving}}function V(r,t){return t.find(e=>e.id===r.group&&e.rooms.some(s=>s.id===r.id))??null}function re(r,t,e){if(!r||!t)return null;let s=V(r,e);return t.kind==="alone"?s?q(r.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?s&&s.id===t.id?null:nt(r.id,t.id):t.kind==="room"?t.id===r.id||s&&s.rooms.some(i=>i.id===t.id)?null:nt(r.id,t.id):null}var $t=r=>r.kind==="alone"?"alone":`${r.kind}:${r.id}`;function se(r){if(r==="alone")return{kind:"alone"};let t=String(r).indexOf(":");if(t<1)return null;let e=r.slice(0,t),s=r.slice(t+1);return(e==="room"||e==="group")&&s?{kind:e,id:s}:null}function ie(r,t){let e=V(r,t);return e?$t({kind:"group",id:e.id}):"alone"}function oe(r,t,e){return[{value:"alone",label:"Alone"},...e.map(s=>({value:$t({kind:"group",id:s.id}),label:s.name})),...t.filter(s=>s.id!==r.id&&!V(s,e)).map(s=>({value:$t({kind:"room",id:s.id}),label:`With ${s.name}`}))]}var ne={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},et=r=>(...t)=>({_$litDirective$:r,values:t}),O=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,s){this._$Ct=t,this._$AM=e,this._$Ci=s}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:Ne}=Qt,ae=r=>r;var le=()=>document.createComment(""),N=(r,t,e)=>{let s=r._$AA.parentNode,i=t===void 0?r._$AB:t._$AA;if(e===void 0){let o=s.insertBefore(le(),i),a=s.insertBefore(le(),i);e=new Ne(o,a,r,r.options)}else{let o=e._$AB.nextSibling,a=e._$AM,d=a!==r;if(d){let l;e._$AQ?.(r),e._$AM=r,e._$AP!==void 0&&(l=r._$AU)!==a._$AU&&e._$AP(l)}if(o!==i||d){let l=e._$AA;for(;l!==o;){let p=ae(l).nextSibling;ae(s).insertBefore(l,i),l=p}}}return e},E=(r,t,e=r)=>(r._$AI(t,e),r),Le={},rt=(r,t=Le)=>r._$AH=t,ue=r=>r._$AH,st=r=>{r._$AR(),r._$AA.remove()};var de=(r,t,e)=>{let s=new Map;for(let i=t;i<=e;i++)s.set(r[i],i);return s},it=et(class extends O{constructor(r){if(super(r),r.type!==ne.CHILD)throw Error("repeat() can only be used in text expressions")}dt(r,t,e){let s;e===void 0?e=t:t!==void 0&&(s=t);let i=[],o=[],a=0;for(let d of r)i[a]=s?s(d,a):a,o[a]=e(d,a),a++;return{values:o,keys:i}}render(r,t,e){return this.dt(r,t,e).values}update(r,[t,e,s]){let i=ue(r),{values:o,keys:a}=this.dt(t,e,s);if(!Array.isArray(i))return this.ut=a,o;let d=this.ut??=[],l=[],p,m,n=0,u=i.length-1,h=0,v=o.length-1;for(;n<=u&&h<=v;)if(i[n]===null)n++;else if(i[u]===null)u--;else if(d[n]===a[h])l[h]=E(i[n],o[h]),n++,h++;else if(d[u]===a[v])l[v]=E(i[u],o[v]),u--,v--;else if(d[n]===a[v])l[v]=E(i[n],o[v]),N(r,l[v+1],i[n]),n++,v--;else if(d[u]===a[h])l[h]=E(i[u],o[h]),N(r,i[n],i[u]),u--,h++;else if(p===void 0&&(p=de(a,h,v),m=de(d,n,u)),p.has(d[n]))if(p.has(d[u])){let y=m.get(a[h]),g=y!==void 0?i[y]:null;if(g===null){let _=N(r,i[n]);E(_,o[h]),l[h]=_}else l[h]=E(g,o[h]),N(r,i[n],g),i[y]=null;h++}else st(i[u]),u--;else st(i[n]),n++;for(;h<=v;){let y=N(r,l[v+1]);E(y,o[h]),l[h++]=y}for(;n<=u;){let y=i[n++];y!==null&&st(y)}return this.ut=a,rt(r,l),k}});var ce=et(class extends O{constructor(){super(...arguments),this.key=c}render(r,t){return this.key=r,t}update(r,[t,e]){return t!==this.key&&(rt(r),this.key=t),e}});var Ue={playing:"Playing",paused:"Paused",buffering:"Buffering"};function He(r,t=[]){if(!r)return"Unavailable";let e=t.find(a=>a.source===r);if(e)return e.label;if(r==="stream")return"The server's stream";if(r==="none")return"Nothing";let[s,...i]=r.split(":"),o=i.join(":");return s==="line-in"&&o?`Input ${o}`:s==="player"&&o?`Network player ${o}`:s==="chime"&&o?`Chime ${o}`:s==="soloist"&&o?"Spotify":r}var _t=class extends ${static properties={target:{type:String},name:{type:String},source:{attribute:!1},nowPlaying:{attribute:!1},inputs:{attribute:!1},pick:{type:Boolean},_failed:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.target="",this.name="",this.source=null,this.nowPlaying=null,this.inputs=[],this.pick=!1,this._failed=null}_onArtworkError(t){this._failed=t.target.getAttribute("src")}_onInput(t){t.source!==this.source&&this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.target,body:Rt(this.target,t.source)},bubbles:!0,composed:!0}))}_artwork(t){let e=f`<span class="placeholder" data-artwork="placeholder" role="img" aria-label="No artwork for ${this.name}"
      >♪</span
    >`;return!t.artwork||t.artwork===this._failed?e:ce(t.artwork,f`<img
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
              <p data-state>${Ue[t.state]??"Unavailable"}</p>
            </div>
          </div>`:c}
      <p class="row" data-source=${this.source??""}>Source: ${He(this.source,e)}</p>
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
    `}};customElements.define("chorus-playing",_t);var ze=r=>`${Math.round(r/10)}%`,yt=class extends ${static properties={group:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.group=null,this.inputs=[],this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let s=t.has("refusal")&&!!this.refusal;s&&(this._dragged=null),(!this._sliderHeld||s)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Ot(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(q(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(o=>o.id)),s=t.defined??[],i=new Set(s.map(o=>o.id));return[...s.map(o=>({...o,playing:e.has(o.id)})),...t.rooms.filter(o=>!i.has(o.id)).map(o=>({...o,playing:!0}))]}render(){let t=this.group;if(!t)return c;let e=t.volume===null?"":ze(this._dragged??t.volume);return f`
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
    `}};customElements.define("chorus-group-card",yt);var bt=class extends ${static properties={groups:{attribute:!1},inputs:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
    `}};customElements.define("chorus-groups",bt);var he=Object.freeze(["app","kiosk"]);function pe(r){let t=new URLSearchParams(r).get("kiosk");return t===null||t==="0"||t==="false"?"app":"kiosk"}var De={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Ie=r=>`${Math.round(r/10)}%`,wt=class extends ${static properties={room:{attribute:!1},inputs:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=b`
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
  `;constructor(){super(),this.room=null,this.inputs=[],this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let s=this._slider;if(!s||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(s.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Tt(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let s=se(e);s&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:s},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(Mt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return c;let e=t.volume===null?"Unavailable":Ie(this._dragged??t.volume);return f`
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
                    ${De[s.role]??s.role}: ${s.name}
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
    `}};customElements.define("chorus-room-card",wt);var At=class extends ${static properties={rooms:{attribute:!1},status:{type:String},inputs:{attribute:!1},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=b`
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
    p[data-status="lost"] {
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
  `;constructor(){super(),this.rooms=null,this.inputs=[],this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],s=this.over;return f`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?f`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:c}
      <ul>
        ${it(t??[],i=>i.id,i=>f`<li
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
                .places=${oe(i,t,e)}
                .place=${ie(i,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",At);var St=class extends ${static properties={mode:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=b`
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
    /* A wall tablet shows the rooms and nothing of the app around them. */
    :host([mode="kiosk"]) header {
      display: none;
    }
  `;constructor(){super(),this.mode="app",this.store=null,this._view={state:null,rooms:[],groups:[],inputs:[],status:"connecting"},this._refusals={},this._moving=null,this._over=null,this._unsubscribe=null,this._drag=ee({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!V(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){he.includes(this.mode)||(this.mode="app"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow()}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""};let s=await this.store.command(e);s.ok||(this._refusals={...this._refusals,[t]:s.refusal})}_onCommand(t){let{subject:e,room:s,body:i}=t.detail;this._send(e??s,i)}_move(t,e){let s=this._room(t),i=re(s,e,this._groups);i&&this._send(t,i)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}render(){return f`
      <header>
        <h1>chorus</h1>
      </header>
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
    `}};customElements.define("chorus-app",St);var je=(r,t)=>at("../",r,t),L=r=>typeof r=="string"&&r?r:null,Be=["playing","paused","buffering"];function me(r,t=je){let e=r&&Array.isArray(r.groups)?r.groups:[],s=new Map;for(let i of e){if(!i||typeof i!="object"||typeof i.id!="string"||!i.id)continue;let o=i.now_playing&&typeof i.now_playing=="object"?i.now_playing:null,a=o?L(o.art_url):null;s.set(i.id,{source:L(i.source),nowPlaying:o&&{title:L(o.title),artist:L(o.artist),album:L(o.album),state:Be.includes(o.state)?o.state:null,via:L(o.via),artwork:a?t(i.id,a):null}})}return s}var Et={source:null,nowPlaying:null};function Ve(r){let t=r&&Array.isArray(r.inputs)?r.inputs:[],e=new Map((r&&Array.isArray(r.input_labels)?r.input_labels:[]).filter(s=>s&&typeof s.input=="string"&&typeof s.name=="string"&&s.name).map(s=>[s.input,s.name]));return t.filter(s=>typeof s=="string"&&s).map(s=>({id:s,source:`line-in:${s}`,label:e.get(s)??s}))}function Fe(r,t,e){if(!r||typeof r!="object"||typeof r.id!="string"||!r.id)return null;let s=Array.isArray(r.bond)?r.bond:[],i=typeof r.group=="string"&&r.group?r.group:r.id;return{id:r.id,name:typeof r.name=="string"&&r.name?r.name:r.id,volume:xt(r.volume),muted:typeof r.muted=="boolean"?r.muted:null,group:i,...i===r.id&&e.get(i)||Et,bond:s.filter(o=>o&&typeof o.endpoint=="string"&&typeof o.role=="string").map(o=>({endpoint:o.endpoint,name:t.get(o.endpoint)??o.endpoint,role:o.role}))}}function fe(r,t){let e=r&&Array.isArray(r.zones)?r.zones:[],s=r&&Array.isArray(r.speakers)?r.speakers:[],i=new Map(s.filter(a=>a&&typeof a.id=="string"&&typeof a.name=="string"&&a.name).map(a=>[a.id,a.name])),o=me(r,t);return e.map(a=>Fe(a,i,o)).filter(Boolean)}var xt=r=>typeof r=="number"&&r>=0&&r<=1?Math.round(r*1e3):null;function qe(r,t){let e=new Map(fe(r,t).map(n=>[n.id,n.name])),s=me(r,t),i=n=>({id:n,name:e.get(n)??n}),o=n=>Array.isArray(n)?n:[],a=n=>o(n).filter(u=>typeof u=="string"&&u).map(i),d=n=>n&&typeof n=="object"&&typeof n.id=="string"&&n.id,l=o(r?.groups).filter(d),p=o(r?.saved_groups).filter(d),m=new Set(p.map(n=>n.id));return[...p.map(n=>{let u=l.find(h=>h.id===n.id);return{id:n.id,name:typeof n.name=="string"&&n.name?n.name:n.id,kind:"saved",active:n.active===!0,defined:a(n.zones),rooms:u?a(u.zones):[],volume:u?xt(u.volume):null,...u&&s.get(n.id)||Et}}),...l.filter(n=>n.kind==="live"&&!m.has(n.id)).map(n=>{let u=a(n.zones);return{id:n.id,name:u.map(h=>h.name).join(" + ")||n.id,kind:"live",active:null,defined:null,rooms:u,volume:xt(n.volume),...s.get(n.id)??Et}})]}var kt=r=>!!r&&typeof r=="object"&&Array.isArray(r.zones);function ge(r){let t=new Set,e=null,s=[],i=[],o=[],a="connecting",d=!1,l=null,p=()=>({state:e,rooms:s,groups:i,inputs:o,status:a}),m=()=>{let g=p();for(let _ of[...t])_(g)},n=g=>{e=g,s=fe(g,r.artwork),i=qe(g,r.artwork),o=Ve(g)};function u(){l||(l=r.events({onState(g){kt(g)&&(d=!0,n(g),m())},onStatus(g){a!==g&&(a=g,m())}}),r.state().then(g=>{d||!kt(g)||(n(g),m())},()=>{}))}function h(){l?.(),l=null}async function v(g){let _=await r.command(g);return _.ok&&kt(_.state)&&(!e||_.state.serial>e.serial)&&(n(_.state),m()),_}function y(g){return t.add(g),g(p()),()=>t.delete(g)}return{start:u,stop:h,command:v,subscribe:y,view:p}}var Ct=document.querySelector("chorus-app");if(Ct){Ct.mode=pe(window.location.search);let r=ge(Nt());Ct.store=r,r.start()}
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
