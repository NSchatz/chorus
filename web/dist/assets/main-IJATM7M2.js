function At(r){let t=Math.min(1e3,Math.max(0,Math.round(Number(r)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function St(r,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(r)},"volume":${At(t)}}`}function wt(r,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(r)},"muted":${t?"true":"false"}}`}function rt(r,t){return`{"v":2,"t":"join","zone":${JSON.stringify(r)},"target":${JSON.stringify(t)}}`}function j(r){return`{"v":2,"t":"take","target":${JSON.stringify(r)}}`}function Et(r,t){return`{"v":2,"t":"group_volume","group":${JSON.stringify(r)},"volume":${At(t)}}`}async function ce(r){let t="";try{t=(await r.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail)return e.detail}catch{}return t||`the server answered ${r.status}`}var he={set:(r,t)=>globalThis.setTimeout(r,t),clear:r=>globalThis.clearTimeout(r)};function xt({fetch:r=globalThis.fetch.bind(globalThis),base:t="../",timers:e=he}={}){async function s(){let n=await r(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store"});if(!n.ok)throw new Error(`the server answered ${n.status}`);return n.json()}async function i(n){let u;try{u=await r(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:n})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(!u.ok)return{ok:!1,refusal:await ce(u)};try{return{ok:!0,state:await u.json()}}catch{return{ok:!0,state:null}}}function o({onState:n,onStatus:u=()=>{}}){let d=!1,l=null,h=null,a=null,c=()=>{a!==null&&e.clear(a),a=null},p=()=>{c(),a=e.set(()=>l?.abort(),4e4)},g=_=>{let E=_.split(`
`).filter(b=>b.startsWith("data:")).map(b=>b.slice(5).replace(/^ /,"")).join(`
`);if(!E)return;let B;try{B=JSON.parse(E)}catch{return}u("live"),n(B)};async function m(){l=new AbortController,p();try{let _=await r(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",signal:l.signal});if(!_.ok||!_.body)throw new Error(`the server answered ${_.status}`);let E=_.body.getReader();l.signal.addEventListener("abort",()=>E.cancel().catch(()=>{}));let B=new TextDecoder,b="";for(;;){let{done:de,value:ue}=await E.read();if(de||d||l.signal.aborted)break;p(),b+=B.decode(ue,{stream:!0}).replace(/\r\n?/g,`
`);let et;for(;(et=b.indexOf(`

`))!==-1;)g(b.slice(0,et)),b=b.slice(et+2)}}catch{}c(),!d&&(u("lost"),h=e.set(()=>{h=null,m()},1e3))}return m(),()=>{d=!0,c(),h!==null&&e.clear(h),l?.abort()}}return{state:s,command:i,events:o}}var V=globalThis,F=V.ShadowRoot&&(V.ShadyCSS===void 0||V.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,st=Symbol(),kt=new WeakMap,N=class{constructor(t,e,s){if(this._$cssResult$=!0,s!==st)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(F&&t===void 0){let s=e!==void 0&&e.length===1;s&&(t=kt.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),s&&kt.set(e,t))}return t}toString(){return this.cssText}},Ct=r=>new N(typeof r=="string"?r:r+"",void 0,st),y=(r,...t)=>{let e=r.length===1?r[0]:t.reduce((s,i,o)=>s+(n=>{if(n._$cssResult$===!0)return n.cssText;if(typeof n=="number")return n;throw Error("Value passed to 'css' function must be a 'css' function result: "+n+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(i)+r[o+1],r[0]);return new N(e,r,st)},Pt=(r,t)=>{if(F)r.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let s=document.createElement("style"),i=V.litNonce;i!==void 0&&s.setAttribute("nonce",i),s.textContent=e.cssText,r.appendChild(s)}},it=F?r=>r:r=>r instanceof CSSStyleSheet?(t=>{let e="";for(let s of t.cssRules)e+=s.cssText;return Ct(e)})(r):r;var{is:pe,defineProperty:me,getOwnPropertyDescriptor:fe,getOwnPropertyNames:ge,getOwnPropertySymbols:ve,getPrototypeOf:_e}=Object,q=globalThis,Tt=q.trustedTypes,$e=Tt?Tt.emptyScript:"",ye=q.reactiveElementPolyfillSupport,H=(r,t)=>r,ot={toAttribute(r,t){switch(t){case Boolean:r=r?$e:null;break;case Object:case Array:r=r==null?r:JSON.stringify(r)}return r},fromAttribute(r,t){let e=r;switch(t){case Boolean:e=r!==null;break;case Number:e=r===null?null:Number(r);break;case Object:case Array:try{e=JSON.parse(r)}catch{e=null}}return e}},Mt=(r,t)=>!pe(r,t),Rt={attribute:!0,type:String,converter:ot,reflect:!1,useDefault:!1,hasChanged:Mt};Symbol.metadata??=Symbol("metadata"),q.litPropertyMetadata??=new WeakMap;var A=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=Rt){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let s=Symbol(),i=this.getPropertyDescriptor(t,s,e);i!==void 0&&me(this.prototype,t,i)}}static getPropertyDescriptor(t,e,s){let{get:i,set:o}=fe(this.prototype,t)??{get(){return this[e]},set(n){this[e]=n}};return{get:i,set(n){let u=i?.call(this);o?.call(this,n),this.requestUpdate(t,u,s)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??Rt}static _$Ei(){if(this.hasOwnProperty(H("elementProperties")))return;let t=_e(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(H("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(H("properties"))){let e=this.properties,s=[...ge(e),...ve(e)];for(let i of s)this.createProperty(i,e[i])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[s,i]of e)this.elementProperties.set(s,i)}this._$Eh=new Map;for(let[e,s]of this.elementProperties){let i=this._$Eu(e,s);i!==void 0&&this._$Eh.set(i,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let s=new Set(t.flat(1/0).reverse());for(let i of s)e.unshift(it(i))}else t!==void 0&&e.push(it(t));return e}static _$Eu(t,e){let s=e.attribute;return s===!1?void 0:typeof s=="string"?s:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let s of e.keys())this.hasOwnProperty(s)&&(t.set(s,this[s]),delete this[s]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return Pt(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,s){this._$AK(t,s)}_$ET(t,e){let s=this.constructor.elementProperties.get(t),i=this.constructor._$Eu(t,s);if(i!==void 0&&s.reflect===!0){let o=(s.converter?.toAttribute!==void 0?s.converter:ot).toAttribute(e,s.type);this._$Em=t,o==null?this.removeAttribute(i):this.setAttribute(i,o),this._$Em=null}}_$AK(t,e){let s=this.constructor,i=s._$Eh.get(t);if(i!==void 0&&this._$Em!==i){let o=s.getPropertyOptions(i),n=typeof o.converter=="function"?{fromAttribute:o.converter}:o.converter?.fromAttribute!==void 0?o.converter:ot;this._$Em=i;let u=n.fromAttribute(e,o.type);this[i]=u??this._$Ej?.get(i)??u,this._$Em=null}}requestUpdate(t,e,s,i=!1,o){if(t!==void 0){let n=this.constructor;if(i===!1&&(o=this[t]),s??=n.getPropertyOptions(t),!((s.hasChanged??Mt)(o,e)||s.useDefault&&s.reflect&&o===this._$Ej?.get(t)&&!this.hasAttribute(n._$Eu(t,s))))return;this.C(t,e,s)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:s,reflect:i,wrapped:o},n){s&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,n??e??this[t]),o!==!0||n!==void 0)||(this._$AL.has(t)||(this.hasUpdated||s||(e=void 0),this._$AL.set(t,e)),i===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[i,o]of this._$Ep)this[i]=o;this._$Ep=void 0}let s=this.constructor.elementProperties;if(s.size>0)for(let[i,o]of s){let{wrapped:n}=o,u=this[i];n!==!0||this._$AL.has(i)||u===void 0||this.C(i,void 0,o,u)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(s=>s.hostUpdate?.()),this.update(e)):this._$EM()}catch(s){throw t=!1,this._$EM(),s}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};A.elementStyles=[],A.shadowRootOptions={mode:"open"},A[H("elementProperties")]=new Map,A[H("finalized")]=new Map,ye?.({ReactiveElement:A}),(q.reactiveElementVersions??=[]).push("2.1.2");var at=globalThis,Ot=r=>r,J=at.trustedTypes,Nt=J?J.createPolicy("lit-html",{createHTML:r=>r}):void 0,lt="$lit$",S=`lit$${Math.random().toFixed(9).slice(2)}$`,dt="?"+S,be=`<${dt}>`,P=document,U=()=>P.createComment(""),D=r=>r===null||typeof r!="object"&&typeof r!="function",ut=Array.isArray,zt=r=>ut(r)||typeof r?.[Symbol.iterator]=="function",nt=`[ 	
\f\r]`,L=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,Ht=/-->/g,Lt=/>/g,k=RegExp(`>|${nt}(?:([^\\s"'>=/]+)(${nt}*=${nt}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),Ut=/'/g,Dt=/"/g,Bt=/^(?:script|style|textarea|title)$/i,ct=r=>(t,...e)=>({_$litType$:r,strings:t,values:e}),v=ct(1),De=ct(2),Ie=ct(3),w=Symbol.for("lit-noChange"),f=Symbol.for("lit-nothing"),It=new WeakMap,C=P.createTreeWalker(P,129);function jt(r,t){if(!ut(r)||!r.hasOwnProperty("raw"))throw Error("invalid template strings array");return Nt!==void 0?Nt.createHTML(t):t}var Vt=(r,t)=>{let e=r.length-1,s=[],i,o=t===2?"<svg>":t===3?"<math>":"",n=L;for(let u=0;u<e;u++){let d=r[u],l,h,a=-1,c=0;for(;c<d.length&&(n.lastIndex=c,h=n.exec(d),h!==null);)c=n.lastIndex,n===L?h[1]==="!--"?n=Ht:h[1]!==void 0?n=Lt:h[2]!==void 0?(Bt.test(h[2])&&(i=RegExp("</"+h[2],"g")),n=k):h[3]!==void 0&&(n=k):n===k?h[0]===">"?(n=i??L,a=-1):h[1]===void 0?a=-2:(a=n.lastIndex-h[2].length,l=h[1],n=h[3]===void 0?k:h[3]==='"'?Dt:Ut):n===Dt||n===Ut?n=k:n===Ht||n===Lt?n=L:(n=k,i=void 0);let p=n===k&&r[u+1].startsWith("/>")?" ":"";o+=n===L?d+be:a>=0?(s.push(l),d.slice(0,a)+lt+d.slice(a)+S+p):d+S+(a===-2?u:p)}return[jt(r,o+(r[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),s]},I=class r{constructor({strings:t,_$litType$:e},s){let i;this.parts=[];let o=0,n=0,u=t.length-1,d=this.parts,[l,h]=Vt(t,e);if(this.el=r.createElement(l,s),C.currentNode=this.el.content,e===2||e===3){let a=this.el.content.firstChild;a.replaceWith(...a.childNodes)}for(;(i=C.nextNode())!==null&&d.length<u;){if(i.nodeType===1){if(i.hasAttributes())for(let a of i.getAttributeNames())if(a.endsWith(lt)){let c=h[n++],p=i.getAttribute(a).split(S),g=/([.?@])?(.*)/.exec(c);d.push({type:1,index:o,name:g[2],strings:p,ctor:g[1]==="."?G:g[1]==="?"?Y:g[1]==="@"?K:R}),i.removeAttribute(a)}else a.startsWith(S)&&(d.push({type:6,index:o}),i.removeAttribute(a));if(Bt.test(i.tagName)){let a=i.textContent.split(S),c=a.length-1;if(c>0){i.textContent=J?J.emptyScript:"";for(let p=0;p<c;p++)i.append(a[p],U()),C.nextNode(),d.push({type:2,index:++o});i.append(a[c],U())}}}else if(i.nodeType===8)if(i.data===dt)d.push({type:2,index:o});else{let a=-1;for(;(a=i.data.indexOf(S,a+1))!==-1;)d.push({type:7,index:o}),a+=S.length-1}o++}}static createElement(t,e){let s=P.createElement("template");return s.innerHTML=t,s}};function T(r,t,e=r,s){if(t===w)return t;let i=s!==void 0?e._$Co?.[s]:e._$Cl,o=D(t)?void 0:t._$litDirective$;return i?.constructor!==o&&(i?._$AO?.(!1),o===void 0?i=void 0:(i=new o(r),i._$AT(r,e,s)),s!==void 0?(e._$Co??=[])[s]=i:e._$Cl=i),i!==void 0&&(t=T(r,i._$AS(r,t.values),i,s)),t}var W=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:s}=this._$AD,i=(t?.creationScope??P).importNode(e,!0);C.currentNode=i;let o=C.nextNode(),n=0,u=0,d=s[0];for(;d!==void 0;){if(n===d.index){let l;d.type===2?l=new M(o,o.nextSibling,this,t):d.type===1?l=new d.ctor(o,d.name,d.strings,this,t):d.type===6&&(l=new X(o,this,t)),this._$AV.push(l),d=s[++u]}n!==d?.index&&(o=C.nextNode(),n++)}return C.currentNode=P,i}p(t){let e=0;for(let s of this._$AV)s!==void 0&&(s.strings!==void 0?(s._$AI(t,s,e),e+=s.strings.length-2):s._$AI(t[e])),e++}},M=class r{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,s,i){this.type=2,this._$AH=f,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=s,this.options=i,this._$Cv=i?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=T(this,t,e),D(t)?t===f||t==null||t===""?(this._$AH!==f&&this._$AR(),this._$AH=f):t!==this._$AH&&t!==w&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):zt(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==f&&D(this._$AH)?this._$AA.nextSibling.data=t:this.T(P.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:s}=t,i=typeof s=="number"?this._$AC(t):(s.el===void 0&&(s.el=I.createElement(jt(s.h,s.h[0]),this.options)),s);if(this._$AH?._$AD===i)this._$AH.p(e);else{let o=new W(i,this),n=o.u(this.options);o.p(e),this.T(n),this._$AH=o}}_$AC(t){let e=It.get(t.strings);return e===void 0&&It.set(t.strings,e=new I(t)),e}k(t){ut(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,s,i=0;for(let o of t)i===e.length?e.push(s=new r(this.O(U()),this.O(U()),this,this.options)):s=e[i],s._$AI(o),i++;i<e.length&&(this._$AR(s&&s._$AB.nextSibling,i),e.length=i)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let s=Ot(t).nextSibling;Ot(t).remove(),t=s}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},R=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,s,i,o){this.type=1,this._$AH=f,this._$AN=void 0,this.element=t,this.name=e,this._$AM=i,this.options=o,s.length>2||s[0]!==""||s[1]!==""?(this._$AH=Array(s.length-1).fill(new String),this.strings=s):this._$AH=f}_$AI(t,e=this,s,i){let o=this.strings,n=!1;if(o===void 0)t=T(this,t,e,0),n=!D(t)||t!==this._$AH&&t!==w,n&&(this._$AH=t);else{let u=t,d,l;for(t=o[0],d=0;d<o.length-1;d++)l=T(this,u[s+d],e,d),l===w&&(l=this._$AH[d]),n||=!D(l)||l!==this._$AH[d],l===f?t=f:t!==f&&(t+=(l??"")+o[d+1]),this._$AH[d]=l}n&&!i&&this.j(t)}j(t){t===f?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},G=class extends R{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===f?void 0:t}},Y=class extends R{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==f)}},K=class extends R{constructor(t,e,s,i,o){super(t,e,s,i,o),this.type=5}_$AI(t,e=this){if((t=T(this,t,e,0)??f)===w)return;let s=this._$AH,i=t===f&&s!==f||t.capture!==s.capture||t.once!==s.once||t.passive!==s.passive,o=t!==f&&(s===f||i);i&&this.element.removeEventListener(this.name,this,s),o&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},X=class{constructor(t,e,s){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=s}get _$AU(){return this._$AM._$AU}_$AI(t){T(this,t)}},Ft={M:lt,P:S,A:dt,C:1,L:Vt,R:W,D:zt,V:T,I:M,H:R,N:Y,U:K,B:G,F:X},Ae=at.litHtmlPolyfillSupport;Ae?.(I,M),(at.litHtmlVersions??=[]).push("3.3.3");var qt=(r,t,e)=>{let s=e?.renderBefore??t,i=s._$litPart$;if(i===void 0){let o=e?.renderBefore??null;s._$litPart$=i=new M(t.insertBefore(U(),o),o,void 0,e??{})}return i._$AI(r),i};var ht=globalThis,$=class extends A{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=qt(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return w}};$._$litElement$=!0,$.finalized=!0,ht.litElementHydrateSupport?.({LitElement:$});var Se=ht.litElementPolyfillSupport;Se?.({LitElement:$});(ht.litElementVersions??=[]).push("4.2.2");function we(r,t,e){let s=r.elementFromPoint?.(t,e)??null;for(;s?.shadowRoot?.elementFromPoint;){let i=s.shadowRoot.elementFromPoint(t,e);if(!i||i===s)break;s=i}return s}function Ee(r){for(let t=r;t;t=t.assignedSlot??t.parentNode??t.host){let e=t.dataset?.drop;if(e==="alone")return{kind:e};if((e==="room"||e==="group")&&t.dataset.dropId)return{kind:e,id:t.dataset.dropId}}return null}var Jt=(r,t,e)=>Ee(we(r,t,e));function Wt({root:r=document,onStart:t=()=>{},onOver:e=()=>{},onEnd:s=()=>{}}={}){let i=null,o=()=>{let{handle:a,pointerId:c}=i;a.removeEventListener("pointermove",n),a.removeEventListener("pointerup",u),a.removeEventListener("pointercancel",d),a.removeEventListener("lostpointercapture",d),r.removeEventListener("keydown",l,!0);try{a.releasePointerCapture?.(c)}catch{}i=null};function n(a){if(!(!i||a.pointerId!==i.pointerId)){if(!i.moving){if(Math.hypot(a.clientX-i.x,a.clientY-i.y)<8)return;i.moving=!0,t(i.room)}a.preventDefault(),e(Jt(r,a.clientX,a.clientY))}}function u(a){if(!i||a.pointerId!==i.pointerId)return;let{room:c,moving:p}=i;if(o(),!p)return;let g=m=>{m.stopPropagation(),m.preventDefault()};r.addEventListener("click",g,!0),setTimeout(()=>r.removeEventListener("click",g,!0),0),s(c,Jt(r,a.clientX,a.clientY))}function d(a){if(!i||a&&a.pointerId!==void 0&&a.pointerId!==i.pointerId)return;let{room:c,moving:p}=i;o(),p&&s(c,null)}function l(a){a.key==="Escape"&&d()}function h(a){if(i||a.isPrimary===!1||a.button>0)return;let c=a.composedPath().find(p=>p.dataset?.dragRoom);if(c){i={handle:c,room:c.dataset.dragRoom,pointerId:a.pointerId,x:a.clientX,y:a.clientY,moving:!1};try{c.setPointerCapture?.(a.pointerId)}catch{}c.addEventListener("pointermove",n),c.addEventListener("pointerup",u),c.addEventListener("pointercancel",d),c.addEventListener("lostpointercapture",d),r.addEventListener("keydown",l,!0)}}return{begin:h,cancel:()=>d(),active:()=>!!i?.moving}}function z(r,t){return t.find(e=>e.id===r.group&&e.rooms.some(s=>s.id===r.id))??null}function Gt(r,t,e){if(!r||!t)return null;let s=z(r,e);return t.kind==="alone"?s?j(r.id):null:typeof t.id!="string"||!t.id?null:t.kind==="group"?s&&s.id===t.id?null:rt(r.id,t.id):t.kind==="room"?t.id===r.id||s&&s.rooms.some(i=>i.id===t.id)?null:rt(r.id,t.id):null}var pt=r=>r.kind==="alone"?"alone":`${r.kind}:${r.id}`;function Yt(r){if(r==="alone")return{kind:"alone"};let t=String(r).indexOf(":");if(t<1)return null;let e=r.slice(0,t),s=r.slice(t+1);return(e==="room"||e==="group")&&s?{kind:e,id:s}:null}function Kt(r,t){let e=z(r,t);return e?pt({kind:"group",id:e.id}):"alone"}function Xt(r,t,e){return[{value:"alone",label:"Alone"},...e.map(s=>({value:pt({kind:"group",id:s.id}),label:s.name})),...t.filter(s=>s.id!==r.id&&!z(s,e)).map(s=>({value:pt({kind:"room",id:s.id}),label:`With ${s.name}`}))]}var Qt={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},Zt=r=>(...t)=>({_$litDirective$:r,values:t}),Q=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,s){this._$Ct=t,this._$AM=e,this._$Ci=s}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:xe}=Ft,te=r=>r;var ee=()=>document.createComment(""),O=(r,t,e)=>{let s=r._$AA.parentNode,i=t===void 0?r._$AB:t._$AA;if(e===void 0){let o=s.insertBefore(ee(),i),n=s.insertBefore(ee(),i);e=new xe(o,n,r,r.options)}else{let o=e._$AB.nextSibling,n=e._$AM,u=n!==r;if(u){let d;e._$AQ?.(r),e._$AM=r,e._$AP!==void 0&&(d=r._$AU)!==n._$AU&&e._$AP(d)}if(o!==i||u){let d=e._$AA;for(;d!==o;){let l=te(d).nextSibling;te(s).insertBefore(d,i),d=l}}}return e},x=(r,t,e=r)=>(r._$AI(t,e),r),ke={},re=(r,t=ke)=>r._$AH=t,se=r=>r._$AH,Z=r=>{r._$AR(),r._$AA.remove()};var ie=(r,t,e)=>{let s=new Map;for(let i=t;i<=e;i++)s.set(r[i],i);return s},tt=Zt(class extends Q{constructor(r){if(super(r),r.type!==Qt.CHILD)throw Error("repeat() can only be used in text expressions")}dt(r,t,e){let s;e===void 0?e=t:t!==void 0&&(s=t);let i=[],o=[],n=0;for(let u of r)i[n]=s?s(u,n):n,o[n]=e(u,n),n++;return{values:o,keys:i}}render(r,t,e){return this.dt(r,t,e).values}update(r,[t,e,s]){let i=se(r),{values:o,keys:n}=this.dt(t,e,s);if(!Array.isArray(i))return this.ut=n,o;let u=this.ut??=[],d=[],l,h,a=0,c=i.length-1,p=0,g=o.length-1;for(;a<=c&&p<=g;)if(i[a]===null)a++;else if(i[c]===null)c--;else if(u[a]===n[p])d[p]=x(i[a],o[p]),a++,p++;else if(u[c]===n[g])d[g]=x(i[c],o[g]),c--,g--;else if(u[a]===n[g])d[g]=x(i[a],o[g]),O(r,d[g+1],i[a]),a++,g--;else if(u[c]===n[p])d[p]=x(i[c],o[p]),O(r,i[a],i[c]),c--,p++;else if(l===void 0&&(l=ie(n,p,g),h=ie(u,a,c)),l.has(u[a]))if(l.has(u[c])){let m=h.get(n[p]),_=m!==void 0?i[m]:null;if(_===null){let E=O(r,i[a]);x(E,o[p]),d[p]=E}else d[p]=x(_,o[p]),O(r,i[a],_),i[m]=null;p++}else Z(i[c]),c--;else Z(i[a]),a++;for(;p<=g;){let m=O(r,d[g+1]);x(m,o[p]),d[p++]=m}for(;a<=c;){let m=i[a++];m!==null&&Z(m)}return this.ut=n,re(r,d),w}});var Ce=r=>`${Math.round(r/10)}%`,mt=class extends ${static properties={group:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.group=null,this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||!this.group||this.group.volume===null)return;let s=t.has("refusal")&&!!this.refusal;s&&(this._dragged=null),(!this._sliderHeld||s)&&(e.value=String(this.group.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{subject:this.group.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this._slider&&this.group.volume!==null&&(this._slider.value=String(this.group.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(Et(this.group.id,Number(t.target.value)))}_onActivate(){this._ask(j(this.group.id))}_onRemove(t){this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:t.id,destination:{kind:"alone"}},bubbles:!0,composed:!0}))}_kindText(){let t=this.group;return t.kind==="live"?"Live group":t.active?"Saved group, active":t.rooms.length>0?"Saved group, partly formed":"Saved group, not active"}_listed(){let t=this.group,e=new Set(t.rooms.map(o=>o.id)),s=t.defined??[],i=new Set(s.map(o=>o.id));return[...s.map(o=>({...o,playing:e.has(o.id)})),...t.rooms.filter(o=>!i.has(o.id)).map(o=>({...o,playing:!0}))]}render(){let t=this.group;if(!t)return f;let e=t.volume===null?"":Ce(this._dragged??t.volume);return v`
      <h2>${t.name}</h2>
      <p data-kind=${t.kind} data-active=${t.active===null?f:String(t.active)}>
        ${this._kindText()}
      </p>
      <ul aria-label="Rooms of ${t.name}">
        ${this._listed().map(s=>v`<li data-member=${s.id} data-playing=${String(s.playing)}>
              <span>${s.name}</span>
              ${s.playing?v`<button
                    type="button"
                    aria-label="Remove ${s.name} from ${t.name}"
                    @click=${()=>this._onRemove(s)}
                  >
                    Remove
                  </button>`:v`<span>Not in the group now</span>`}
            </li>`)}
      </ul>
      ${t.kind==="saved"&&!t.active?v`<div class="row">
            <button type="button" aria-label="Group the rooms of ${t.name}" @click=${this._onActivate}>
              Group these rooms
            </button>
          </div>`:f}
      ${t.volume===null?f:v`<div class="row">
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
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:f}</p>
    `}};customElements.define("chorus-group-card",mt);var ft=class extends ${static properties={groups:{attribute:!1},refusals:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=y`
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
  `;constructor(){super(),this.groups=null,this.refusals={},this.moving=null,this.over=null}render(){let t=this.groups??[],e=this.over;return v`
      ${this.groups!==null&&t.length===0?v`<p data-empty>No groups yet. Drag a room onto another room to play them together.</p>`:f}
      <ul>
        ${tt(t,s=>s.id,s=>v`<li
              data-group=${s.id}
              data-drop="group"
              data-drop-id=${s.id}
              ?data-over=${e?.kind==="group"&&e.id===s.id}
            >
              <chorus-group-card .group=${s} .refusal=${this.refusals[s.id]??""}></chorus-group-card>
            </li>`)}
      </ul>
      <p data-drop="alone" ?hidden=${!this.moving?.grouped} ?data-over=${e?.kind==="alone"}>
        ${this.moving?`Drop here to play ${this.moving.name} alone.`:f}
      </p>
    `}};customElements.define("chorus-groups",ft);var oe=Object.freeze(["app","kiosk"]);function ne(r){let t=new URLSearchParams(r).get("kiosk");return t===null||t==="0"||t==="false"?"app":"kiosk"}var Pe={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},Te=r=>`${Math.round(r/10)}%`,gt=class extends ${static properties={room:{attribute:!1},refusal:{type:String},places:{attribute:!1},place:{type:String},_dragged:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.room=null,this.refusal="",this.places=[],this.place="alone",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._list;e&&(e.value=this.place);let s=this._slider;if(!s||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(s.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(St(this.room.id,Number(t.target.value)))}get _list(){return this.renderRoot.querySelector("select")}_onPlace(t){let e=t.target.value;if(t.target.value=this.place,e===this.place)return;let s=Yt(e);s&&this.dispatchEvent(new CustomEvent("chorus-move",{detail:{room:this.room.id,destination:s},bubbles:!0,composed:!0}))}_onHandle(){this._list?.focus()}_onMute(){this._ask(wt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return f;let e=t.volume===null?"Unavailable":Te(this._dragged??t.volume);return v`
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
      ${t.bond.length===0?f:v`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(s=>v`<li data-endpoint=${s.endpoint} data-role=${s.role}>
                    ${Pe[s.role]??s.role}: ${s.name}
                  </li>`)}
            </ul>
          `}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?f:v`<input
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
          ${this.places.map(s=>v`<option value=${s.value} ?selected=${s.value===this.place}>${s.label}</option>`)}
        </select>
      </div>
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:f}</p>
    `}};customElements.define("chorus-room-card",gt);var vt=class extends ${static properties={rooms:{attribute:!1},status:{type:String},refusals:{attribute:!1},groups:{attribute:!1},moving:{attribute:!1},over:{attribute:!1}};static styles=y`
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
  `;constructor(){super(),this.rooms=null,this.status="connecting",this.refusals={},this.groups=[],this.moving=null,this.over=null}_statusText(){return this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms,e=this.groups??[],s=this.over;return v`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?v`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:f}
      <ul>
        ${tt(t??[],i=>i.id,i=>v`<li
              data-room=${i.id}
              data-drop="room"
              data-drop-id=${i.id}
              ?data-moving=${this.moving?.id===i.id}
              ?data-over=${s?.kind==="room"&&s.id===i.id&&this.moving?.id!==i.id}
            >
              <chorus-room-card
                .room=${i}
                .refusal=${this.refusals[i.id]??""}
                .places=${Xt(i,t,e)}
                .place=${Kt(i,e)}
              ></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",vt);var _t=class extends ${static properties={mode:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0},_moving:{state:!0},_over:{state:!0}};static styles=y`
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
  `;constructor(){super(),this.mode="app",this.store=null,this._view={state:null,rooms:[],groups:[],status:"connecting"},this._refusals={},this._moving=null,this._over=null,this._unsubscribe=null,this._drag=Wt({onStart:t=>{let e=this._room(t);e&&(this._moving={id:t,name:e.name,grouped:!!z(e,this._groups)})},onOver:t=>{let e=this._over;e?.kind===t?.kind&&e?.id===t?.id||(this._over=t)},onEnd:(t,e)=>{this._moving=null,this._over=null,e&&this._move(t,e)}})}get _groups(){return this._view.groups??[]}_room(t){return this._view.rooms.find(e=>e.id===t)??null}willUpdate(t){oe.includes(this.mode)||(this.mode="app"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow()}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null,this._drag.cancel()}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _send(t,e){if(!this.store)return;this._refusals={...this._refusals,[t]:""};let s=await this.store.command(e);s.ok||(this._refusals={...this._refusals,[t]:s.refusal})}_onCommand(t){let{subject:e,room:s,body:i}=t.detail;this._send(e??s,i)}_move(t,e){let s=this._room(t),i=Gt(s,e,this._groups);i&&this._send(t,i)}_onMove(t){this._move(t.detail.room,t.detail.destination)}_onPointerDown(t){this._drag.begin(t)}render(){return v`
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
          .refusals=${this._refusals}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-groups>
        <p role="status" data-drag>
          ${this._moving?`Moving ${this._moving.name}. Drop it on a room or a group.`:f}
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
          .refusals=${this._refusals}
          .groups=${this._groups}
          .moving=${this._moving}
          .over=${this._over}
        ></chorus-rooms>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",_t);function Re(r,t){if(!r||typeof r!="object"||typeof r.id!="string"||!r.id)return null;let e=Array.isArray(r.bond)?r.bond:[];return{id:r.id,name:typeof r.name=="string"&&r.name?r.name:r.id,volume:yt(r.volume),muted:typeof r.muted=="boolean"?r.muted:null,group:typeof r.group=="string"&&r.group?r.group:r.id,bond:e.filter(s=>s&&typeof s.endpoint=="string"&&typeof s.role=="string").map(s=>({endpoint:s.endpoint,name:t.get(s.endpoint)??s.endpoint,role:s.role}))}}function ae(r){let t=r&&Array.isArray(r.zones)?r.zones:[],e=r&&Array.isArray(r.speakers)?r.speakers:[],s=new Map(e.filter(i=>i&&typeof i.id=="string"&&typeof i.name=="string"&&i.name).map(i=>[i.id,i.name]));return t.map(i=>Re(i,s)).filter(Boolean)}var yt=r=>typeof r=="number"&&r>=0&&r<=1?Math.round(r*1e3):null;function Me(r){let t=new Map(ae(r).map(l=>[l.id,l.name])),e=l=>({id:l,name:t.get(l)??l}),s=l=>Array.isArray(l)?l:[],i=l=>s(l).filter(h=>typeof h=="string"&&h).map(e),o=l=>l&&typeof l=="object"&&typeof l.id=="string"&&l.id,n=s(r?.groups).filter(o),u=s(r?.saved_groups).filter(o),d=new Set(u.map(l=>l.id));return[...u.map(l=>{let h=n.find(a=>a.id===l.id);return{id:l.id,name:typeof l.name=="string"&&l.name?l.name:l.id,kind:"saved",active:l.active===!0,defined:i(l.zones),rooms:h?i(h.zones):[],volume:h?yt(h.volume):null}}),...n.filter(l=>l.kind==="live"&&!d.has(l.id)).map(l=>{let h=i(l.zones);return{id:l.id,name:h.map(a=>a.name).join(" + ")||l.id,kind:"live",active:null,defined:null,rooms:h,volume:yt(l.volume)}})]}var $t=r=>!!r&&typeof r=="object"&&Array.isArray(r.zones);function le(r){let t=new Set,e=null,s=[],i=[],o="connecting",n=!1,u=null,d=()=>({state:e,rooms:s,groups:i,status:o}),l=()=>{let m=d();for(let _ of[...t])_(m)},h=m=>{e=m,s=ae(m),i=Me(m)};function a(){u||(u=r.events({onState(m){$t(m)&&(n=!0,h(m),l())},onStatus(m){o!==m&&(o=m,l())}}),r.state().then(m=>{n||!$t(m)||(h(m),l())},()=>{}))}function c(){u?.(),u=null}async function p(m){let _=await r.command(m);return _.ok&&$t(_.state)&&(!e||_.state.serial>e.serial)&&(h(_.state),l()),_}function g(m){return t.add(m),m(d()),()=>t.delete(m)}return{start:a,stop:c,command:p,subscribe:g,view:d}}var bt=document.querySelector("chorus-app");if(bt){bt.mode=ne(window.location.search);let r=le(xt());bt.store=r,r.start()}
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
*/
