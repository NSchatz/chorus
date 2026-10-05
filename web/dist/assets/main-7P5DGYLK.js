function Yt(s){let t=Math.min(1e3,Math.max(0,Math.round(Number(s)||0)));return`${Math.floor(t/1e3)}.${String(t%1e3).padStart(3,"0")}`}function ft(s,t){return`{"v":1,"t":"volume","zone":${JSON.stringify(s)},"volume":${Yt(t)}}`}function mt(s,t){return`{"v":1,"t":"mute","zone":${JSON.stringify(s)},"muted":${t?"true":"false"}}`}async function Gt(s){let t="";try{t=(await s.text()).trim()}catch{t=""}try{let e=JSON.parse(t);if(e&&typeof e.detail=="string"&&e.detail)return e.detail}catch{}return t||`the server answered ${s.status}`}var Qt={set:(s,t)=>globalThis.setTimeout(s,t),clear:s=>globalThis.clearTimeout(s)};function $t({fetch:s=globalThis.fetch.bind(globalThis),base:t="../",timers:e=Qt}={}){async function i(){let o=await s(`${t}api/state`,{headers:{Accept:"application/json"},cache:"no-store"});if(!o.ok)throw new Error(`the server answered ${o.status}`);return o.json()}async function r(o){let h;try{h=await s(`${t}api/command`,{method:"POST",headers:{"Content-Type":"application/json"},body:o})}catch{return{ok:!1,refusal:"the server could not be reached"}}if(!h.ok)return{ok:!1,refusal:await Gt(h)};try{return{ok:!0,state:await h.json()}}catch{return{ok:!0,state:null}}}function n({onState:o,onStatus:h=()=>{}}){let a=!1,d=null,f=null,l=null,p=()=>{l!==null&&e.clear(l),l=null},u=()=>{p(),l=e.set(()=>d?.abort(),4e4)},c=g=>{let E=g.split(`
`).filter(y=>y.startsWith("data:")).map(y=>y.slice(5).replace(/^ /,"")).join(`
`);if(!E)return;let j;try{j=JSON.parse(E)}catch{return}h("live"),o(j)};async function $(){d=new AbortController,u();try{let g=await s(`${t}api/events`,{headers:{Accept:"text/event-stream"},cache:"no-store",signal:d.signal});if(!g.ok||!g.body)throw new Error(`the server answered ${g.status}`);let E=g.body.getReader();d.signal.addEventListener("abort",()=>E.cancel().catch(()=>{}));let j=new TextDecoder,y="";for(;;){let{done:Jt,value:Kt}=await E.read();if(Jt||a||d.signal.aborted)break;u(),y+=j.decode(Kt,{stream:!0}).replace(/\r\n?/g,`
`);let Z;for(;(Z=y.indexOf(`

`))!==-1;)c(y.slice(0,Z)),y=y.slice(Z+2)}}catch{}p(),!a&&(h("lost"),f=e.set(()=>{f=null,$()},1e3))}return $(),()=>{a=!0,p(),f!==null&&e.clear(f),d?.abort()}}return{state:i,command:r,events:n}}var z=globalThis,I=z.ShadowRoot&&(z.ShadyCSS===void 0||z.ShadyCSS.nativeShadow)&&"adoptedStyleSheets"in Document.prototype&&"replace"in CSSStyleSheet.prototype,X=Symbol(),_t=new WeakMap,O=class{constructor(t,e,i){if(this._$cssResult$=!0,i!==X)throw Error("CSSResult is not constructable. Use `unsafeCSS` or `css` instead.");this.cssText=t,this.t=e}get styleSheet(){let t=this.o,e=this.t;if(I&&t===void 0){let i=e!==void 0&&e.length===1;i&&(t=_t.get(e)),t===void 0&&((this.o=t=new CSSStyleSheet).replaceSync(this.cssText),i&&_t.set(e,t))}return t}toString(){return this.cssText}},vt=s=>new O(typeof s=="string"?s:s+"",void 0,X),C=(s,...t)=>{let e=s.length===1?s[0]:t.reduce((i,r,n)=>i+(o=>{if(o._$cssResult$===!0)return o.cssText;if(typeof o=="number")return o;throw Error("Value passed to 'css' function must be a 'css' function result: "+o+". Use 'unsafeCSS' to pass non-literal values, but take care to ensure page security.")})(r)+s[n+1],s[0]);return new O(e,s,X)},gt=(s,t)=>{if(I)s.adoptedStyleSheets=t.map(e=>e instanceof CSSStyleSheet?e:e.styleSheet);else for(let e of t){let i=document.createElement("style"),r=z.litNonce;r!==void 0&&i.setAttribute("nonce",r),i.textContent=e.cssText,s.appendChild(i)}},tt=I?s=>s:s=>s instanceof CSSStyleSheet?(t=>{let e="";for(let i of t.cssRules)e+=i.cssText;return vt(e)})(s):s;var{is:Zt,defineProperty:Xt,getOwnPropertyDescriptor:te,getOwnPropertyNames:ee,getOwnPropertySymbols:se,getPrototypeOf:re}=Object,V=globalThis,yt=V.trustedTypes,ie=yt?yt.emptyScript:"",oe=V.reactiveElementPolyfillSupport,N=(s,t)=>s,et={toAttribute(s,t){switch(t){case Boolean:s=s?ie:null;break;case Object:case Array:s=s==null?s:JSON.stringify(s)}return s},fromAttribute(s,t){let e=s;switch(t){case Boolean:e=s!==null;break;case Number:e=s===null?null:Number(s);break;case Object:case Array:try{e=JSON.parse(s)}catch{e=null}}return e}},bt=(s,t)=>!Zt(s,t),At={attribute:!0,type:String,converter:et,reflect:!1,useDefault:!1,hasChanged:bt};Symbol.metadata??=Symbol("metadata"),V.litPropertyMetadata??=new WeakMap;var A=class extends HTMLElement{static addInitializer(t){this._$Ei(),(this.l??=[]).push(t)}static get observedAttributes(){return this.finalize(),this._$Eh&&[...this._$Eh.keys()]}static createProperty(t,e=At){if(e.state&&(e.attribute=!1),this._$Ei(),this.prototype.hasOwnProperty(t)&&((e=Object.create(e)).wrapped=!0),this.elementProperties.set(t,e),!e.noAccessor){let i=Symbol(),r=this.getPropertyDescriptor(t,i,e);r!==void 0&&Xt(this.prototype,t,r)}}static getPropertyDescriptor(t,e,i){let{get:r,set:n}=te(this.prototype,t)??{get(){return this[e]},set(o){this[e]=o}};return{get:r,set(o){let h=r?.call(this);n?.call(this,o),this.requestUpdate(t,h,i)},configurable:!0,enumerable:!0}}static getPropertyOptions(t){return this.elementProperties.get(t)??At}static _$Ei(){if(this.hasOwnProperty(N("elementProperties")))return;let t=re(this);t.finalize(),t.l!==void 0&&(this.l=[...t.l]),this.elementProperties=new Map(t.elementProperties)}static finalize(){if(this.hasOwnProperty(N("finalized")))return;if(this.finalized=!0,this._$Ei(),this.hasOwnProperty(N("properties"))){let e=this.properties,i=[...ee(e),...se(e)];for(let r of i)this.createProperty(r,e[r])}let t=this[Symbol.metadata];if(t!==null){let e=litPropertyMetadata.get(t);if(e!==void 0)for(let[i,r]of e)this.elementProperties.set(i,r)}this._$Eh=new Map;for(let[e,i]of this.elementProperties){let r=this._$Eu(e,i);r!==void 0&&this._$Eh.set(r,e)}this.elementStyles=this.finalizeStyles(this.styles)}static finalizeStyles(t){let e=[];if(Array.isArray(t)){let i=new Set(t.flat(1/0).reverse());for(let r of i)e.unshift(tt(r))}else t!==void 0&&e.push(tt(t));return e}static _$Eu(t,e){let i=e.attribute;return i===!1?void 0:typeof i=="string"?i:typeof t=="string"?t.toLowerCase():void 0}constructor(){super(),this._$Ep=void 0,this.isUpdatePending=!1,this.hasUpdated=!1,this._$Em=null,this._$Ev()}_$Ev(){this._$ES=new Promise(t=>this.enableUpdating=t),this._$AL=new Map,this._$E_(),this.requestUpdate(),this.constructor.l?.forEach(t=>t(this))}addController(t){(this._$EO??=new Set).add(t),this.renderRoot!==void 0&&this.isConnected&&t.hostConnected?.()}removeController(t){this._$EO?.delete(t)}_$E_(){let t=new Map,e=this.constructor.elementProperties;for(let i of e.keys())this.hasOwnProperty(i)&&(t.set(i,this[i]),delete this[i]);t.size>0&&(this._$Ep=t)}createRenderRoot(){let t=this.shadowRoot??this.attachShadow(this.constructor.shadowRootOptions);return gt(t,this.constructor.elementStyles),t}connectedCallback(){this.renderRoot??=this.createRenderRoot(),this.enableUpdating(!0),this._$EO?.forEach(t=>t.hostConnected?.())}enableUpdating(t){}disconnectedCallback(){this._$EO?.forEach(t=>t.hostDisconnected?.())}attributeChangedCallback(t,e,i){this._$AK(t,i)}_$ET(t,e){let i=this.constructor.elementProperties.get(t),r=this.constructor._$Eu(t,i);if(r!==void 0&&i.reflect===!0){let n=(i.converter?.toAttribute!==void 0?i.converter:et).toAttribute(e,i.type);this._$Em=t,n==null?this.removeAttribute(r):this.setAttribute(r,n),this._$Em=null}}_$AK(t,e){let i=this.constructor,r=i._$Eh.get(t);if(r!==void 0&&this._$Em!==r){let n=i.getPropertyOptions(r),o=typeof n.converter=="function"?{fromAttribute:n.converter}:n.converter?.fromAttribute!==void 0?n.converter:et;this._$Em=r;let h=o.fromAttribute(e,n.type);this[r]=h??this._$Ej?.get(r)??h,this._$Em=null}}requestUpdate(t,e,i,r=!1,n){if(t!==void 0){let o=this.constructor;if(r===!1&&(n=this[t]),i??=o.getPropertyOptions(t),!((i.hasChanged??bt)(n,e)||i.useDefault&&i.reflect&&n===this._$Ej?.get(t)&&!this.hasAttribute(o._$Eu(t,i))))return;this.C(t,e,i)}this.isUpdatePending===!1&&(this._$ES=this._$EP())}C(t,e,{useDefault:i,reflect:r,wrapped:n},o){i&&!(this._$Ej??=new Map).has(t)&&(this._$Ej.set(t,o??e??this[t]),n!==!0||o!==void 0)||(this._$AL.has(t)||(this.hasUpdated||i||(e=void 0),this._$AL.set(t,e)),r===!0&&this._$Em!==t&&(this._$Eq??=new Set).add(t))}async _$EP(){this.isUpdatePending=!0;try{await this._$ES}catch(e){Promise.reject(e)}let t=this.scheduleUpdate();return t!=null&&await t,!this.isUpdatePending}scheduleUpdate(){return this.performUpdate()}performUpdate(){if(!this.isUpdatePending)return;if(!this.hasUpdated){if(this.renderRoot??=this.createRenderRoot(),this._$Ep){for(let[r,n]of this._$Ep)this[r]=n;this._$Ep=void 0}let i=this.constructor.elementProperties;if(i.size>0)for(let[r,n]of i){let{wrapped:o}=n,h=this[r];o!==!0||this._$AL.has(r)||h===void 0||this.C(r,void 0,n,h)}}let t=!1,e=this._$AL;try{t=this.shouldUpdate(e),t?(this.willUpdate(e),this._$EO?.forEach(i=>i.hostUpdate?.()),this.update(e)):this._$EM()}catch(i){throw t=!1,this._$EM(),i}t&&this._$AE(e)}willUpdate(t){}_$AE(t){this._$EO?.forEach(e=>e.hostUpdated?.()),this.hasUpdated||(this.hasUpdated=!0,this.firstUpdated(t)),this.updated(t)}_$EM(){this._$AL=new Map,this.isUpdatePending=!1}get updateComplete(){return this.getUpdateComplete()}getUpdateComplete(){return this._$ES}shouldUpdate(t){return!0}update(t){this._$Eq&&=this._$Eq.forEach(e=>this._$ET(e,this[e])),this._$EM()}updated(t){}firstUpdated(t){}};A.elementStyles=[],A.shadowRootOptions={mode:"open"},A[N("elementProperties")]=new Map,A[N("finalized")]=new Map,oe?.({ReactiveElement:A}),(V.reactiveElementVersions??=[]).push("2.1.2");var rt=globalThis,St=s=>s,q=rt.trustedTypes,Et=q?q.createPolicy("lit-html",{createHTML:s=>s}):void 0,it="$lit$",b=`lit$${Math.random().toFixed(9).slice(2)}$`,ot="?"+b,ne=`<${ot}>`,P=document,L=()=>P.createComment(""),B=s=>s===null||typeof s!="object"&&typeof s!="function",nt=Array.isArray,kt=s=>nt(s)||typeof s?.[Symbol.iterator]=="function",st=`[ 	
\f\r]`,H=/<(?:(!--|\/[^a-zA-Z])|(\/?[a-zA-Z][^>\s]*)|(\/?$))/g,wt=/-->/g,Ct=/>/g,x=RegExp(`>|${st}(?:([^\\s"'>=/]+)(${st}*=${st}*(?:[^ 	
\f\r"'\`<>=]|("|')|))|$)`,"g"),xt=/'/g,Tt=/"/g,Mt=/^(?:script|style|textarea|title)$/i,at=s=>(t,...e)=>({_$litType$:s,strings:t,values:e}),v=at(1),ye=at(2),Ae=at(3),S=Symbol.for("lit-noChange"),m=Symbol.for("lit-nothing"),Pt=new WeakMap,T=P.createTreeWalker(P,129);function Rt(s,t){if(!nt(s)||!s.hasOwnProperty("raw"))throw Error("invalid template strings array");return Et!==void 0?Et.createHTML(t):t}var Ut=(s,t)=>{let e=s.length-1,i=[],r,n=t===2?"<svg>":t===3?"<math>":"",o=H;for(let h=0;h<e;h++){let a=s[h],d,f,l=-1,p=0;for(;p<a.length&&(o.lastIndex=p,f=o.exec(a),f!==null);)p=o.lastIndex,o===H?f[1]==="!--"?o=wt:f[1]!==void 0?o=Ct:f[2]!==void 0?(Mt.test(f[2])&&(r=RegExp("</"+f[2],"g")),o=x):f[3]!==void 0&&(o=x):o===x?f[0]===">"?(o=r??H,l=-1):f[1]===void 0?l=-2:(l=o.lastIndex-f[2].length,d=f[1],o=f[3]===void 0?x:f[3]==='"'?Tt:xt):o===Tt||o===xt?o=x:o===wt||o===Ct?o=H:(o=x,r=void 0);let u=o===x&&s[h+1].startsWith("/>")?" ":"";n+=o===H?a+ne:l>=0?(i.push(d),a.slice(0,l)+it+a.slice(l)+b+u):a+b+(l===-2?h:u)}return[Rt(s,n+(s[e]||"<?>")+(t===2?"</svg>":t===3?"</math>":"")),i]},D=class s{constructor({strings:t,_$litType$:e},i){let r;this.parts=[];let n=0,o=0,h=t.length-1,a=this.parts,[d,f]=Ut(t,e);if(this.el=s.createElement(d,i),T.currentNode=this.el.content,e===2||e===3){let l=this.el.content.firstChild;l.replaceWith(...l.childNodes)}for(;(r=T.nextNode())!==null&&a.length<h;){if(r.nodeType===1){if(r.hasAttributes())for(let l of r.getAttributeNames())if(l.endsWith(it)){let p=f[o++],u=r.getAttribute(l).split(b),c=/([.?@])?(.*)/.exec(p);a.push({type:1,index:n,name:c[2],strings:u,ctor:c[1]==="."?W:c[1]==="?"?J:c[1]==="@"?K:M}),r.removeAttribute(l)}else l.startsWith(b)&&(a.push({type:6,index:n}),r.removeAttribute(l));if(Mt.test(r.tagName)){let l=r.textContent.split(b),p=l.length-1;if(p>0){r.textContent=q?q.emptyScript:"";for(let u=0;u<p;u++)r.append(l[u],L()),T.nextNode(),a.push({type:2,index:++n});r.append(l[p],L())}}}else if(r.nodeType===8)if(r.data===ot)a.push({type:2,index:n});else{let l=-1;for(;(l=r.data.indexOf(b,l+1))!==-1;)a.push({type:7,index:n}),l+=b.length-1}n++}}static createElement(t,e){let i=P.createElement("template");return i.innerHTML=t,i}};function k(s,t,e=s,i){if(t===S)return t;let r=i!==void 0?e._$Co?.[i]:e._$Cl,n=B(t)?void 0:t._$litDirective$;return r?.constructor!==n&&(r?._$AO?.(!1),n===void 0?r=void 0:(r=new n(s),r._$AT(s,e,i)),i!==void 0?(e._$Co??=[])[i]=r:e._$Cl=r),r!==void 0&&(t=k(s,r._$AS(s,t.values),r,i)),t}var F=class{constructor(t,e){this._$AV=[],this._$AN=void 0,this._$AD=t,this._$AM=e}get parentNode(){return this._$AM.parentNode}get _$AU(){return this._$AM._$AU}u(t){let{el:{content:e},parts:i}=this._$AD,r=(t?.creationScope??P).importNode(e,!0);T.currentNode=r;let n=T.nextNode(),o=0,h=0,a=i[0];for(;a!==void 0;){if(o===a.index){let d;a.type===2?d=new R(n,n.nextSibling,this,t):a.type===1?d=new a.ctor(n,a.name,a.strings,this,t):a.type===6&&(d=new Y(n,this,t)),this._$AV.push(d),a=i[++h]}o!==a?.index&&(n=T.nextNode(),o++)}return T.currentNode=P,r}p(t){let e=0;for(let i of this._$AV)i!==void 0&&(i.strings!==void 0?(i._$AI(t,i,e),e+=i.strings.length-2):i._$AI(t[e])),e++}},R=class s{get _$AU(){return this._$AM?._$AU??this._$Cv}constructor(t,e,i,r){this.type=2,this._$AH=m,this._$AN=void 0,this._$AA=t,this._$AB=e,this._$AM=i,this.options=r,this._$Cv=r?.isConnected??!0}get parentNode(){let t=this._$AA.parentNode,e=this._$AM;return e!==void 0&&t?.nodeType===11&&(t=e.parentNode),t}get startNode(){return this._$AA}get endNode(){return this._$AB}_$AI(t,e=this){t=k(this,t,e),B(t)?t===m||t==null||t===""?(this._$AH!==m&&this._$AR(),this._$AH=m):t!==this._$AH&&t!==S&&this._(t):t._$litType$!==void 0?this.$(t):t.nodeType!==void 0?this.T(t):kt(t)?this.k(t):this._(t)}O(t){return this._$AA.parentNode.insertBefore(t,this._$AB)}T(t){this._$AH!==t&&(this._$AR(),this._$AH=this.O(t))}_(t){this._$AH!==m&&B(this._$AH)?this._$AA.nextSibling.data=t:this.T(P.createTextNode(t)),this._$AH=t}$(t){let{values:e,_$litType$:i}=t,r=typeof i=="number"?this._$AC(t):(i.el===void 0&&(i.el=D.createElement(Rt(i.h,i.h[0]),this.options)),i);if(this._$AH?._$AD===r)this._$AH.p(e);else{let n=new F(r,this),o=n.u(this.options);n.p(e),this.T(o),this._$AH=n}}_$AC(t){let e=Pt.get(t.strings);return e===void 0&&Pt.set(t.strings,e=new D(t)),e}k(t){nt(this._$AH)||(this._$AH=[],this._$AR());let e=this._$AH,i,r=0;for(let n of t)r===e.length?e.push(i=new s(this.O(L()),this.O(L()),this,this.options)):i=e[r],i._$AI(n),r++;r<e.length&&(this._$AR(i&&i._$AB.nextSibling,r),e.length=r)}_$AR(t=this._$AA.nextSibling,e){for(this._$AP?.(!1,!0,e);t!==this._$AB;){let i=St(t).nextSibling;St(t).remove(),t=i}}setConnected(t){this._$AM===void 0&&(this._$Cv=t,this._$AP?.(t))}},M=class{get tagName(){return this.element.tagName}get _$AU(){return this._$AM._$AU}constructor(t,e,i,r,n){this.type=1,this._$AH=m,this._$AN=void 0,this.element=t,this.name=e,this._$AM=r,this.options=n,i.length>2||i[0]!==""||i[1]!==""?(this._$AH=Array(i.length-1).fill(new String),this.strings=i):this._$AH=m}_$AI(t,e=this,i,r){let n=this.strings,o=!1;if(n===void 0)t=k(this,t,e,0),o=!B(t)||t!==this._$AH&&t!==S,o&&(this._$AH=t);else{let h=t,a,d;for(t=n[0],a=0;a<n.length-1;a++)d=k(this,h[i+a],e,a),d===S&&(d=this._$AH[a]),o||=!B(d)||d!==this._$AH[a],d===m?t=m:t!==m&&(t+=(d??"")+n[a+1]),this._$AH[a]=d}o&&!r&&this.j(t)}j(t){t===m?this.element.removeAttribute(this.name):this.element.setAttribute(this.name,t??"")}},W=class extends M{constructor(){super(...arguments),this.type=3}j(t){this.element[this.name]=t===m?void 0:t}},J=class extends M{constructor(){super(...arguments),this.type=4}j(t){this.element.toggleAttribute(this.name,!!t&&t!==m)}},K=class extends M{constructor(t,e,i,r,n){super(t,e,i,r,n),this.type=5}_$AI(t,e=this){if((t=k(this,t,e,0)??m)===S)return;let i=this._$AH,r=t===m&&i!==m||t.capture!==i.capture||t.once!==i.once||t.passive!==i.passive,n=t!==m&&(i===m||r);r&&this.element.removeEventListener(this.name,this,i),n&&this.element.addEventListener(this.name,this,t),this._$AH=t}handleEvent(t){typeof this._$AH=="function"?this._$AH.call(this.options?.host??this.element,t):this._$AH.handleEvent(t)}},Y=class{constructor(t,e,i){this.element=t,this.type=6,this._$AN=void 0,this._$AM=e,this.options=i}get _$AU(){return this._$AM._$AU}_$AI(t){k(this,t)}},Ot={M:it,P:b,A:ot,C:1,L:Ut,R:F,D:kt,V:k,I:R,H:M,N:J,U:K,B:W,F:Y},ae=rt.litHtmlPolyfillSupport;ae?.(D,R),(rt.litHtmlVersions??=[]).push("3.3.3");var Nt=(s,t,e)=>{let i=e?.renderBefore??t,r=i._$litPart$;if(r===void 0){let n=e?.renderBefore??null;i._$litPart$=r=new R(t.insertBefore(L(),n),n,void 0,e??{})}return r._$AI(s),r};var lt=globalThis,_=class extends A{constructor(){super(...arguments),this.renderOptions={host:this},this._$Do=void 0}createRenderRoot(){let t=super.createRenderRoot();return this.renderOptions.renderBefore??=t.firstChild,t}update(t){let e=this.render();this.hasUpdated||(this.renderOptions.isConnected=this.isConnected),super.update(t),this._$Do=Nt(e,this.renderRoot,this.renderOptions)}connectedCallback(){super.connectedCallback(),this._$Do?.setConnected(!0)}disconnectedCallback(){super.disconnectedCallback(),this._$Do?.setConnected(!1)}render(){return S}};_._$litElement$=!0,_.finalized=!0,lt.litElementHydrateSupport?.({LitElement:_});var le=lt.litElementPolyfillSupport;le?.({LitElement:_});(lt.litElementVersions??=[]).push("4.2.2");var Ht=Object.freeze(["app","kiosk"]);function Lt(s){let t=new URLSearchParams(s).get("kiosk");return t===null||t==="0"||t==="false"?"app":"kiosk"}var Bt={ATTRIBUTE:1,CHILD:2,PROPERTY:3,BOOLEAN_ATTRIBUTE:4,EVENT:5,ELEMENT:6},Dt=s=>(...t)=>({_$litDirective$:s,values:t}),G=class{constructor(t){}get _$AU(){return this._$AM._$AU}_$AT(t,e,i){this._$Ct=t,this._$AM=e,this._$Ci=i}_$AS(t,e){return this.update(t,e)}update(t,e){return this.render(...e)}};var{I:he}=Ot,jt=s=>s;var zt=()=>document.createComment(""),U=(s,t,e)=>{let i=s._$AA.parentNode,r=t===void 0?s._$AB:t._$AA;if(e===void 0){let n=i.insertBefore(zt(),r),o=i.insertBefore(zt(),r);e=new he(n,o,s,s.options)}else{let n=e._$AB.nextSibling,o=e._$AM,h=o!==s;if(h){let a;e._$AQ?.(s),e._$AM=s,e._$AP!==void 0&&(a=s._$AU)!==o._$AU&&e._$AP(a)}if(n!==r||h){let a=e._$AA;for(;a!==n;){let d=jt(a).nextSibling;jt(i).insertBefore(a,r),a=d}}}return e},w=(s,t,e=s)=>(s._$AI(t,e),s),ce={},It=(s,t=ce)=>s._$AH=t,Vt=s=>s._$AH,Q=s=>{s._$AR(),s._$AA.remove()};var qt=(s,t,e)=>{let i=new Map;for(let r=t;r<=e;r++)i.set(s[r],r);return i},Ft=Dt(class extends G{constructor(s){if(super(s),s.type!==Bt.CHILD)throw Error("repeat() can only be used in text expressions")}dt(s,t,e){let i;e===void 0?e=t:t!==void 0&&(i=t);let r=[],n=[],o=0;for(let h of s)r[o]=i?i(h,o):o,n[o]=e(h,o),o++;return{values:n,keys:r}}render(s,t,e){return this.dt(s,t,e).values}update(s,[t,e,i]){let r=Vt(s),{values:n,keys:o}=this.dt(t,e,i);if(!Array.isArray(r))return this.ut=o,n;let h=this.ut??=[],a=[],d,f,l=0,p=r.length-1,u=0,c=n.length-1;for(;l<=p&&u<=c;)if(r[l]===null)l++;else if(r[p]===null)p--;else if(h[l]===o[u])a[u]=w(r[l],n[u]),l++,u++;else if(h[p]===o[c])a[c]=w(r[p],n[c]),p--,c--;else if(h[l]===o[c])a[c]=w(r[l],n[c]),U(s,a[c+1],r[l]),l++,c--;else if(h[p]===o[u])a[u]=w(r[p],n[u]),U(s,r[l],r[p]),p--,u++;else if(d===void 0&&(d=qt(o,u,c),f=qt(h,l,p)),d.has(h[l]))if(d.has(h[p])){let $=f.get(o[u]),g=$!==void 0?r[$]:null;if(g===null){let E=U(s,r[l]);w(E,n[u]),a[u]=E}else a[u]=w(g,n[u]),U(s,r[l],g),r[$]=null;u++}else Q(r[p]),p--;else Q(r[l]),l++;for(;u<=c;){let $=U(s,a[c+1]);w($,n[u]),a[u++]=$}for(;l<=p;){let $=r[l++];$!==null&&Q($)}return this.ut=o,It(s,a),S}});var de={FL:"Front left",FR:"Front right",FC:"Centre",LFE:"Subwoofer",BL:"Rear left",BR:"Rear right",SL:"Surround left",SR:"Surround right"},ue=s=>`${Math.round(s/10)}%`,ht=class extends _{static properties={room:{attribute:!1},refusal:{type:String},_dragged:{state:!0}};static styles=C`
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
    button:focus-visible {
      outline: var(--focus-ring-width) solid var(--focus);
      outline-offset: var(--focus-ring-offset);
    }
    [role="alert"] {
      margin: var(--reset-margin);
      color: var(--bad);
    }
  `;constructor(){super(),this.room=null,this.refusal="",this._dragged=null,this._sliderHeld=!1}get _slider(){return this.renderRoot.querySelector("input[type=range]")}updated(t){let e=this._slider;if(!e||this.room.volume===null)return;let i=t.has("refusal")&&!!this.refusal;i&&(this._dragged=null),(!this._sliderHeld||i)&&(e.value=String(this.room.volume))}_ask(t){this.dispatchEvent(new CustomEvent("chorus-command",{detail:{room:this.room.id,body:t},bubbles:!0,composed:!0}))}_onSliderFocus(){this._sliderHeld=!0}_onSliderBlur(){this._sliderHeld=!1,this._dragged=null,this.room.volume!==null&&(this._slider.value=String(this.room.volume))}_onSliderInput(t){this._dragged=Number(t.target.value)}_onSliderChange(t){this._dragged=null,this._ask(ft(this.room.id,Number(t.target.value)))}_onMute(){this._ask(mt(this.room.id,!this.room.muted))}render(){let t=this.room;if(!t)return m;let e=t.volume===null?"Unavailable":ue(this._dragged??t.volume);return v`
      <h2>${t.name}</h2>
      ${t.bond.length===0?m:v`
            <h3 id="bond">Bonded set</h3>
            <ul aria-labelledby="bond">
              ${t.bond.map(i=>v`<li data-endpoint=${i.endpoint} data-role=${i.role}>
                    ${de[i.role]??i.role}: ${i.name}
                  </li>`)}
            </ul>
          `}
      <div class="row">
        <label for="volume">Volume</label>
        ${t.volume===null?m:v`<input
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
      <p role="alert">${this.refusal?`Refused: ${this.refusal}`:m}</p>
    `}};customElements.define("chorus-room-card",ht);var ct=class extends _{static properties={rooms:{attribute:!1},status:{type:String},refusals:{attribute:!1}};static styles=C`
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
    code {
      font-family: var(--face-figure);
      font-size: var(--code-size);
    }
  `;constructor(){super(),this.rooms=null,this.status="connecting",this.refusals={}}_statusText(){return this.status==="lost"?this.rooms===null?"The server cannot be reached.":"Connection lost. This is the last known state.":this.rooms===null?"Reading this server's rooms.":""}render(){let t=this.rooms;return v`
      <p role="status" data-status=${this.status}>${this._statusText()}</p>
      ${t!==null&&t.length===0?v`<p data-empty>
            No rooms yet. Start the server with one <code>--zone</code> for each room.
          </p>`:m}
      <ul>
        ${Ft(t??[],e=>e.id,e=>v`<li data-room=${e.id}>
              <chorus-room-card .room=${e} .refusal=${this.refusals[e.id]??""}></chorus-room-card>
            </li>`)}
      </ul>
    `}};customElements.define("chorus-rooms",ct);var dt=class extends _{static properties={mode:{type:String,reflect:!0},store:{attribute:!1},_view:{state:!0},_refusals:{state:!0}};static styles=C`
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
    main {
      padding: var(--surface-pad);
    }
    /* A wall tablet shows the rooms and nothing of the app around them. */
    :host([mode="kiosk"]) header {
      display: none;
    }
  `;constructor(){super(),this.mode="app",this.store=null,this._view={state:null,rooms:[],status:"connecting"},this._refusals={},this._unsubscribe=null}willUpdate(t){Ht.includes(this.mode)||(this.mode="app"),t.has("store")&&this._follow()}connectedCallback(){super.connectedCallback(),this._follow()}disconnectedCallback(){super.disconnectedCallback(),this._unsubscribe?.(),this._unsubscribe=null}_follow(){this._unsubscribe?.(),this._unsubscribe=null,!(!this.store||!this.isConnected)&&(this._unsubscribe=this.store.subscribe(t=>{this._view=t}))}async _onCommand(t){let{room:e,body:i}=t.detail;if(!this.store)return;this._refusals={...this._refusals,[e]:""};let r=await this.store.command(i);r.ok||(this._refusals={...this._refusals,[e]:r.refusal})}render(){return v`
      <header>
        <h1>chorus</h1>
      </header>
      <main aria-label="Rooms" @chorus-command=${this._onCommand}>
        <chorus-rooms
          .rooms=${this._view.state===null?null:this._view.rooms}
          .status=${this._view.status}
          .refusals=${this._refusals}
        ></chorus-rooms>
        <slot></slot>
      </main>
    `}};customElements.define("chorus-app",dt);function pe(s,t){if(!s||typeof s!="object"||typeof s.id!="string"||!s.id)return null;let e=s.volume,i=Array.isArray(s.bond)?s.bond:[];return{id:s.id,name:typeof s.name=="string"&&s.name?s.name:s.id,volume:typeof e=="number"&&e>=0&&e<=1?Math.round(e*1e3):null,muted:typeof s.muted=="boolean"?s.muted:null,bond:i.filter(r=>r&&typeof r.endpoint=="string"&&typeof r.role=="string").map(r=>({endpoint:r.endpoint,name:t.get(r.endpoint)??r.endpoint,role:r.role}))}}function fe(s){let t=s&&Array.isArray(s.zones)?s.zones:[],e=s&&Array.isArray(s.speakers)?s.speakers:[],i=new Map(e.filter(r=>r&&typeof r.id=="string"&&typeof r.name=="string"&&r.name).map(r=>[r.id,r.name]));return t.map(r=>pe(r,i)).filter(Boolean)}var ut=s=>!!s&&typeof s=="object"&&Array.isArray(s.zones);function Wt(s){let t=new Set,e=null,i=[],r="connecting",n=!1,o=null,h=()=>({state:e,rooms:i,status:r}),a=()=>{let c=h();for(let $ of[...t])$(c)},d=c=>{e=c,i=fe(c)};function f(){o||(o=s.events({onState(c){ut(c)&&(n=!0,d(c),a())},onStatus(c){r!==c&&(r=c,a())}}),s.state().then(c=>{n||!ut(c)||(d(c),a())},()=>{}))}function l(){o?.(),o=null}async function p(c){let $=await s.command(c);return $.ok&&ut($.state)&&(!e||$.state.serial>e.serial)&&(d($.state),a()),$}function u(c){return t.add(c),c(h()),()=>t.delete(c)}return{start:f,stop:l,command:p,subscribe:u,view:h}}var pt=document.querySelector("chorus-app");if(pt){pt.mode=Lt(window.location.search);let s=Wt($t());pt.store=s,s.start()}
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
