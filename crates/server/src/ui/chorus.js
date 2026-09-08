// The control page.
//
// It holds no state of its own. Everything it shows comes from the state
// message the server fans out, so what a person sees is what the server says
// and never what this page last did. A command is sent, and the page changes
// when the state message that resulted comes back down the event stream, which
// is the same message every other subscriber gets.
//
// Three rules run through the whole file, and each is graded by a rendered
// assertion in tools/ui/ui.spec.js:
//
//   A figure that cannot be read renders as the word "Unavailable", never as
//   0, a dash, NaN, undefined or a blank, and one such figure costs nothing
//   else on the page.
//
//   A figure computed over rows says which rows it counted and how many it
//   left out, beside the figure, in a few words.
//
//   A figure whose feed has dropped reads as last known rather than as
//   current, and says so within a second of the drop rather than freezing.
//
// The volume literal is built by hand rather than with JSON.stringify, because
// the catalog declares exactly three fractional digits and JSON.stringify would
// write 0.5 where the catalog says 0.500. docs/control-plane.md is the
// contract; fixtures/control/volume.json is what it looks like.

(function () {
  "use strict";

  var DOC = "/docs/control-page.md";

  var zonesEl = document.querySelector("[data-zones]");
  var connectionEl = document.querySelector("[data-connection]");
  var serialEl = document.querySelector("[data-serial]");
  var footerFreshnessEl = document.querySelector('[data-freshness="footer"]');

  /// The last state message this page could read, and nothing else. Every
  /// figure on the page is painted from this, so there is no second copy that
  /// could disagree with it.
  var lastState = null;
  /// Whether the feed is delivering. False means every figure is last known.
  var fresh = false;
  /// The event stream, and whether it has ever been open. A stream that has not
  /// opened YET on a page that has just loaded is not a dropped one.
  var events = null;
  var everOpened = false;
  var secondsWithoutStream = 0;
  /// Per zone, the field of the last command the server refused. Kept here
  /// rather than in the DOM so that a repaint cannot silently drop it.
  var refusals = {};
  /// What is rendered right now: "loading", "empty", "error" or "zones". Only
  /// ever one of them.
  var showing = "loading";
  /// The zone cards currently rendered, by zone id.
  var cards = {};
  var renderedIds = [];
  /// Zones the state carried that this page could not identify at all. Counted
  /// and reported rather than dropped in silence: a row left out for want of a
  /// value is still a row.
  var unreadableZones = 0;

  // --- reading the state, and admitting when it cannot be read ---------------

  /// A volume, or null when the field is absent or is not a number this page
  /// can turn into a percentage. Null is what makes the figure render as the
  /// word "Unavailable" instead of as a zero somebody would read as silence.
  function readVolume(value) {
    if (typeof value !== "number" || !isFinite(value) || value < 0 || value > 1) {
      return null;
    }
    return value;
  }

  /// A list of endpoint identifiers, or null when the field is absent or is not
  /// a list.
  function readList(value) {
    if (!Array.isArray(value)) {
      return null;
    }
    for (var i = 0; i < value.length; i += 1) {
      if (typeof value[i] !== "string") {
        return null;
      }
    }
    return value;
  }

  function readText(value) {
    return typeof value === "string" && value.length > 0 ? value : null;
  }

  /// One zone, as this page can read it. Every field that could not be read is
  /// null, and null is rendered in words.
  function readZone(raw) {
    if (!raw || typeof raw !== "object") {
      return null;
    }
    var id = readText(raw.id);
    if (id === null) {
      return null;
    }
    var name = readText(raw.name);
    return {
      id: id,
      name: name === null ? id : name,
      nameReadable: name !== null,
      group: readText(raw.group),
      volume: readVolume(raw.volume),
      muted: typeof raw.muted === "boolean" ? raw.muted : null,
      present: readList(raw.present),
      endpoints: readList(raw.endpoints),
      audio: readText(raw.audio)
    };
  }

  function percentText(thousandths) {
    return Math.round(thousandths / 10) + "%";
  }

  function volumeLiteral(thousandths) {
    var whole = Math.floor(thousandths / 1000);
    var fraction = String(thousandths % 1000).padStart(3, "0");
    return whole + "." + fraction;
  }

  /// The endpoint figure, which always says which rows it counted. A count with
  /// no set beside it is a count a reader takes for the whole set, and the
  /// persisted list is routinely larger than the attached one.
  function endpointFigure(zone) {
    if (zone.present === null || zone.endpoints === null) {
      return "Endpoints unavailable";
    }
    var away = zone.endpoints.length - zone.present.length;
    var text =
      zone.present.length + " of " + zone.endpoints.length + " endpoints attached";
    if (away > 0) {
      text += ", " + away + " away";
    }
    return text;
  }

  // --- the DOM ---------------------------------------------------------------

  function element(tag, attributes, text) {
    var node = document.createElement(tag);
    Object.keys(attributes || {}).forEach(function (key) {
      node.setAttribute(key, attributes[key]);
    });
    if (text !== undefined) {
      node.textContent = text;
    }
    return node;
  }

  /// The one link a region carries to the document that explains it. The
  /// paragraphs are in that document; what stays here is a few words.
  function docLink(text, label) {
    var link = element("a", { class: "doc-link", href: DOC }, text);
    if (label) {
      link.setAttribute("aria-label", label);
    }
    return link;
  }

  function clear(node) {
    while (node.firstChild) {
      node.removeChild(node.firstChild);
    }
  }

  function emptyNotice() {
    var box = element("section", {
      class: "notice",
      "data-empty": "",
      "data-region": "empty"
    });
    box.appendChild(element("h2", {}, "No zones yet"));
    box.appendChild(
      element("p", {}, "Start the server with one --zone for each room:")
    );
    box.appendChild(
      element(
        "code",
        {},
        "chorus-server --control-listen 127.0.0.1:4020 --zone kitchen --zone study"
      )
    );
    box.appendChild(element("p", {}, "Restart it and this page shows them."));
    box.appendChild(docLink("How zones are configured"));
    return box;
  }

  function errorNotice() {
    var box = element("section", {
      class: "notice",
      "data-error": "",
      "data-region": "error"
    });
    box.appendChild(element("h2", {}, "State could not be read"));
    box.appendChild(element("p", {}, "This page cannot read the server's state."));
    box.appendChild(element("p", {}, "Check chorus-server is running, then reload."));
    box.appendChild(docLink("What this page shows"));
    return box;
  }

  /// Exactly one of loading, empty, error and the zone list is on the page.
  /// Two at once is the failure this function exists to make impossible.
  function show(kind, build) {
    showing = kind;
    clear(zonesEl);
    cards = {};
    renderedIds = [];
    if (build) {
      zonesEl.appendChild(build());
    }
  }

  // --- one zone card ---------------------------------------------------------

  function groupsOf(zones) {
    var groups = [];
    zones.forEach(function (zone) {
      [zone.group, zone.id].forEach(function (group) {
        if (group !== null && groups.indexOf(group) === -1) {
          groups.push(group);
        }
      });
    });
    groups.sort();
    return groups;
  }

  function command(zoneId, body) {
    delete refusals[zoneId];
    paintRefusal(zoneId);
    return fetch("/api/command", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body
    })
      .then(function (response) {
        if (response.ok) {
          return;
        }
        return response.text().then(function (text) {
          var field = "the message";
          try {
            var answer = JSON.parse(text);
            if (typeof answer.field === "string" && answer.field.length > 0) {
              field = answer.field;
            }
          } catch (e) {
            field = "the message";
          }
          refuse(zoneId, field);
        });
      })
      .catch(function () {
        refuse(zoneId, "the server");
      });
  }

  /// A refused command is shown where the person who issued it is looking, and
  /// the control it came from goes back to the state the server still holds.
  /// The words stay short; what a refusal means is in the document this region
  /// links to.
  function refuse(zoneId, field) {
    refusals[zoneId] = field;
    paintZones(true);
  }

  function paintRefusal(zoneId) {
    var card = cards[zoneId];
    if (!card) {
      return;
    }
    card.refusal.textContent = refusals[zoneId]
      ? "Refused: " + refusals[zoneId]
      : "";
  }

  function buildCard(zone, groups) {
    var card = element("section", {
      class: "zone",
      "data-zone": zone.id,
      "data-region": "zone-" + zone.id
    });

    var heading = element("h2", { "data-zone-name": zone.id });
    card.appendChild(heading);

    var meta = element("p", { class: "meta", "data-zone-meta": zone.id });
    card.appendChild(meta);

    var endpoints = element("p", { class: "meta", "data-endpoints": zone.id });
    card.appendChild(endpoints);

    var freshnessLine = element("p", { class: "meta" }, "Figures ");
    var freshness = element("span", { "data-freshness": zone.id }, "live");
    freshnessLine.appendChild(freshness);
    card.appendChild(freshnessLine);

    var volumeRow = element("div", { class: "row" });
    volumeRow.appendChild(
      element("label", { for: "volume-" + zone.id }, "Volume")
    );
    var slider = element("input", {
      type: "range",
      id: "volume-" + zone.id,
      min: "0",
      max: "1000",
      step: "1",
      "data-volume-slider": zone.id
    });
    var readout = element("span", { class: "value", "data-volume": zone.id });
    slider.addEventListener("input", function () {
      readout.textContent = percentText(Number(slider.value));
    });
    slider.addEventListener("change", function () {
      command(
        zone.id,
        '{"v":1,"t":"volume","zone":"' +
          zone.id +
          '","volume":' +
          volumeLiteral(Number(slider.value)) +
          "}"
      );
    });
    volumeRow.appendChild(slider);
    volumeRow.appendChild(readout);
    card.appendChild(volumeRow);

    var muteRow = element("div", { class: "row" });
    muteRow.appendChild(element("label", {}, "Mute"));
    var mute = element(
      "button",
      { type: "button", "data-mute": zone.id },
      "Mute"
    );
    var muteState = element("span", {
      class: "value",
      "data-mute-state": zone.id
    });
    mute.addEventListener("click", function () {
      var current = mute.getAttribute("aria-pressed") === "true";
      command(
        zone.id,
        '{"v":1,"t":"mute","zone":"' +
          zone.id +
          '","muted":' +
          (current ? "false" : "true") +
          "}"
      );
    });
    muteRow.appendChild(mute);
    muteRow.appendChild(muteState);
    card.appendChild(muteRow);

    var nameRow = element("div", { class: "row" });
    nameRow.appendChild(element("label", { for: "name-" + zone.id }, "Name"));
    var rename = element("input", {
      type: "text",
      id: "name-" + zone.id,
      "data-rename": zone.id
    });
    rename.addEventListener("change", function () {
      var wanted = rename.value.trim();
      if (wanted.length === 0) {
        rename.value = currentName(zone.id);
        return;
      }
      command(
        zone.id,
        '{"v":1,"t":"name","zone":"' +
          zone.id +
          '","name":' +
          JSON.stringify(wanted) +
          "}"
      );
    });
    nameRow.appendChild(rename);
    card.appendChild(nameRow);

    var groupRow = element("div", { class: "row" });
    groupRow.appendChild(element("label", { for: "group-" + zone.id }, "Group"));
    var select = element("select", {
      id: "group-" + zone.id,
      "data-group-select": zone.id
    });
    select.addEventListener("change", function () {
      command(
        zone.id,
        '{"v":1,"t":"group","zone":"' +
          zone.id +
          '","group":"' +
          select.value +
          '"}'
      );
    });
    groupRow.appendChild(select);
    var ungroup = element(
      "button",
      { type: "button", "data-ungroup": zone.id },
      "Ungroup"
    );
    ungroup.addEventListener("click", function () {
      command(zone.id, '{"v":1,"t":"ungroup","zone":"' + zone.id + '"}');
    });
    groupRow.appendChild(ungroup);
    card.appendChild(groupRow);

    var refusal = element("p", {
      class: "refusal",
      "data-refusal": zone.id,
      role: "alert"
    });
    card.appendChild(refusal);

    var link = docLink("What these figures mean");
    card.appendChild(link);

    cards[zone.id] = {
      card: card,
      heading: heading,
      meta: meta,
      endpoints: endpoints,
      freshness: freshness,
      volumeRow: volumeRow,
      slider: slider,
      readout: readout,
      mute: mute,
      muteState: muteState,
      rename: rename,
      select: select,
      ungroup: ungroup,
      refusal: refusal,
      link: link
    };
    fillCard(zone, groups, true);
    return card;
  }

  function currentName(zoneId) {
    if (!lastState) {
      return "";
    }
    var found = "";
    lastState.zones.forEach(function (zone) {
      if (zone.id === zoneId) {
        found = zone.name;
      }
    });
    return found;
  }

  /// Put this zone's figures on its card. `force` overwrites a control the
  /// person is holding, which is what a refusal needs and what an ordinary
  /// state message must not do.
  function fillCard(zone, groups, force) {
    var parts = cards[zone.id];
    parts.heading.textContent = zone.name;

    var meta = "id " + zone.id;
    meta += " · group " + (zone.group === null ? "unavailable" : zone.group);
    meta += " · stream " + (zone.audio === null ? "unavailable" : zone.audio);
    parts.meta.textContent = meta;

    parts.endpoints.textContent = endpointFigure(zone);

    // Volume. A volume that cannot be read gets no slider at all: a slider has
    // to be somewhere, and anywhere it could be is a value this page would be
    // making up. It is removed rather than hidden, because a control painted at
    // nothing is still a control a finger cannot hit.
    if (zone.volume === null) {
      if (parts.slider.parentNode) {
        parts.slider.parentNode.removeChild(parts.slider);
      }
      parts.readout.textContent = "Unavailable";
    } else {
      if (!parts.slider.parentNode) {
        parts.volumeRow.insertBefore(parts.slider, parts.readout);
      }
      parts.slider.setAttribute("aria-label", "Volume for " + zone.name);
      if (force || document.activeElement !== parts.slider) {
        parts.slider.value = String(Math.round(zone.volume * 1000));
      }
      parts.readout.textContent = percentText(Number(parts.slider.value));
    }

    // Mute, as a pressed state and as a word. The state is on the button so
    // something that is not reading the page can tell; the word is beside it so
    // a rendering with no colour still can.
    if (zone.muted === null) {
      parts.mute.setAttribute("aria-pressed", "false");
      parts.mute.disabled = true;
      parts.muteState.textContent = "Unavailable";
    } else {
      parts.mute.disabled = false;
      parts.mute.setAttribute("aria-pressed", zone.muted ? "true" : "false");
      parts.muteState.textContent = zone.muted ? "Muted" : "Not muted";
    }
    parts.mute.setAttribute("aria-label", "Mute " + zone.name);

    parts.rename.setAttribute("aria-label", "Name for " + zone.name);
    if (force || document.activeElement !== parts.rename) {
      parts.rename.value = zone.name;
    }

    parts.select.setAttribute("aria-label", "Group for " + zone.name);
    var wantedGroups = groups.join(",");
    if (parts.select.getAttribute("data-groups") !== wantedGroups) {
      parts.select.setAttribute("data-groups", wantedGroups);
      clear(parts.select);
      groups.forEach(function (group) {
        parts.select.appendChild(element("option", { value: group }, group));
      });
    }
    if (zone.group !== null) {
      parts.select.value = zone.group;
    }
    parts.select.disabled = zone.group === null;

    parts.ungroup.setAttribute("aria-label", "Ungroup " + zone.name);
    parts.link.setAttribute(
      "aria-label",
      "What these figures mean for " + zone.name
    );
    parts.freshness.textContent = fresh ? "live" : "last known";
    paintRefusal(zone.id);
  }

  // --- painting the whole page ----------------------------------------------

  function paintFreshness() {
    if (lastState === null) {
      return;
    }
    connectionEl.textContent = fresh ? "Live" : "Connection lost";
    footerFreshnessEl.textContent = fresh ? "live" : "last known";
    Object.keys(cards).forEach(function (id) {
      cards[id].freshness.textContent = fresh ? "live" : "last known";
    });
  }

  function setFresh(value) {
    if (fresh === value) {
      return;
    }
    fresh = value;
    paintFreshness();
  }

  function paintZones(force) {
    var zones = lastState.zones;
    if (zones.length === 0) {
      if (showing !== "empty") {
        show("empty", emptyNotice);
      }
      return;
    }
    var ids = zones.map(function (zone) {
      return zone.id;
    });
    var groups = groupsOf(zones);
    if (showing !== "zones" || ids.join(",") !== renderedIds.join(",")) {
      show("zones", null);
      showing = "zones";
      renderedIds = ids;
      zones.forEach(function (zone) {
        zonesEl.appendChild(buildCard(zone, groups));
      });
      return;
    }
    zones.forEach(function (zone) {
      fillCard(zone, groups, force === true);
    });
  }

  function paintSerial(state) {
    var text = "state " + state.serial + " · catalog version " + state.v;
    if (unreadableZones > 0) {
      text += " · " + unreadableZones + " zones unreadable";
    }
    serialEl.textContent = text;
  }

  /// One state message, applied. Everything the page shows is repainted from
  /// it, so there is no path by which a figure keeps a value the state no
  /// longer carries.
  function apply(raw) {
    if (!raw || typeof raw !== "object" || !Array.isArray(raw.zones)) {
      throw new Error("the state message has no zone list");
    }
    var zones = [];
    unreadableZones = 0;
    raw.zones.forEach(function (entry) {
      var zone = readZone(entry);
      if (zone !== null) {
        zones.push(zone);
      } else {
        unreadableZones += 1;
      }
    });
    lastState = { serial: raw.serial, v: raw.v, zones: zones };
    paintZones(false);
    paintSerial(raw);
    paintFreshness();
  }

  function fail() {
    lastState = null;
    show("error", errorNotice);
    connectionEl.textContent = "Connection lost";
    serialEl.textContent = "No state";
    footerFreshnessEl.textContent = "not yet";
  }

  // --- the feed ---------------------------------------------------------------

  function live() {
    events = new EventSource("/api/events");
    events.onopen = function () {
      everOpened = true;
      secondsWithoutStream = 0;
      setFresh(true);
    };
    events.onmessage = function (event) {
      try {
        apply(JSON.parse(event.data));
        setFresh(true);
      } catch (e) {
        fail();
      }
    };
    events.onerror = function () {
      // A dropped event stream is not a reason to keep showing a stale page as
      // live. EventSource reconnects by itself; what changes here is what the
      // page admits to while it is gone.
      if (everOpened) {
        setFresh(false);
      }
    };
    window.setInterval(function () {
      if (events.readyState === 1) {
        everOpened = true;
        secondsWithoutStream = 0;
        setFresh(true);
        return;
      }
      if (everOpened) {
        setFresh(false);
        return;
      }
      secondsWithoutStream += 1;
      if (secondsWithoutStream >= 5) {
        setFresh(false);
      }
    }, 1000);
  }

  // The state is fetched once as well as subscribed to, so the page shows
  // something even where the event stream cannot be opened at all.
  fetch("/api/state")
    .then(function (response) {
      if (!response.ok) {
        throw new Error("the server answered " + response.status);
      }
      return response.json();
    })
    .then(apply)
    .catch(fail)
    .finally(live);
})();
