// The control page.
//
// It holds no state of its own. Everything it shows comes from the state
// message the server fans out, so what a person sees is what the server says
// and never what this page last did. A command is sent, and the page changes
// when the state message that resulted comes back down the event stream, which
// is the same message every other subscriber gets.
//
// The volume literal is built by hand rather than with JSON.stringify, because
// the catalog declares exactly three fractional digits and JSON.stringify would
// write 0.5 where the catalog says 0.500. docs/control-plane.md is the
// contract; fixtures/control/volume.json is what it looks like.

(function () {
  "use strict";

  var zonesEl = document.querySelector("[data-zones]");
  var connectionEl = document.querySelector("[data-connection]");
  var serialEl = document.querySelector("[data-serial]");
  var lastSerial = -1;

  function send(body) {
    return fetch("/api/command", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: body
    });
  }

  function volumeLiteral(thousandths) {
    var whole = Math.floor(thousandths / 1000);
    var fraction = String(thousandths % 1000).padStart(3, "0");
    return whole + "." + fraction;
  }

  function percentText(thousandths) {
    return Math.round(thousandths / 10) + "%";
  }

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

  function emptyState() {
    var box = element("section", { class: "empty", "data-empty": "" });
    box.appendChild(element("h2", {}, "No zones yet"));
    box.appendChild(
      element(
        "p",
        {},
        "This server has no zone configured, so there is nothing to name, " +
          "group or turn down. Zones are configured when the server starts, " +
          "one --zone for each room:"
      )
    );
    box.appendChild(
      element(
        "code",
        {},
        "chorus-server --control-listen 127.0.0.1:4020 --zone kitchen --zone study"
      )
    );
    box.appendChild(
      element(
        "p",
        {},
        "Restart the server with those and this page will show them, with no " +
          "further configuration and nothing to click here first."
      )
    );
    return box;
  }

  function groupRow(zone, everyGroup) {
    var row = element("div", { class: "row" });
    row.appendChild(element("label", { for: "group-" + zone.id }, "Group"));
    var select = element("select", {
      id: "group-" + zone.id,
      "data-group-select": zone.id,
      "aria-label": "Group for " + zone.name
    });
    everyGroup.forEach(function (group) {
      var option = element("option", { value: group }, group);
      if (group === zone.group) {
        option.setAttribute("selected", "selected");
      }
      select.appendChild(option);
    });
    select.addEventListener("change", function () {
      send(
        '{"v":1,"t":"group","zone":"' +
          zone.id +
          '","group":"' +
          select.value +
          '"}'
      );
    });
    row.appendChild(select);
    var ungroup = element(
      "button",
      { type: "button", "data-ungroup": zone.id },
      "Ungroup"
    );
    ungroup.addEventListener("click", function () {
      send('{"v":1,"t":"ungroup","zone":"' + zone.id + '"}');
    });
    row.appendChild(ungroup);
    return row;
  }

  function volumeRow(zone) {
    var row = element("div", { class: "row" });
    row.appendChild(element("label", { for: "volume-" + zone.id }, "Volume"));
    var slider = element("input", {
      type: "range",
      id: "volume-" + zone.id,
      min: "0",
      max: "1000",
      step: "1",
      value: String(Math.round(zone.volume * 1000)),
      "data-volume-slider": zone.id,
      "aria-label": "Volume for " + zone.name
    });
    var readout = element(
      "span",
      { class: "value", "data-volume": zone.id },
      percentText(Math.round(zone.volume * 1000))
    );
    slider.addEventListener("input", function () {
      readout.textContent = percentText(Number(slider.value));
    });
    slider.addEventListener("change", function () {
      send(
        '{"v":1,"t":"volume","zone":"' +
          zone.id +
          '","volume":' +
          volumeLiteral(Number(slider.value)) +
          "}"
      );
    });
    row.appendChild(slider);
    row.appendChild(readout);
    return row;
  }

  function muteRow(zone) {
    var row = element("div", { class: "row" });
    row.appendChild(element("label", {}, "Mute"));
    var button = element(
      "button",
      {
        type: "button",
        "data-mute": zone.id,
        "aria-pressed": zone.muted ? "true" : "false"
      },
      zone.muted ? "Muted" : "Not muted"
    );
    button.addEventListener("click", function () {
      send(
        '{"v":1,"t":"mute","zone":"' +
          zone.id +
          '","muted":' +
          (zone.muted ? "false" : "true") +
          "}"
      );
    });
    row.appendChild(button);
    var name = element("input", {
      type: "text",
      value: zone.name,
      "data-rename": zone.id,
      "aria-label": "Name for " + zone.id
    });
    name.addEventListener("change", function () {
      var wanted = name.value.trim();
      if (wanted.length === 0) {
        name.value = zone.name;
        return;
      }
      send(
        '{"v":1,"t":"name","zone":"' +
          zone.id +
          '","name":' +
          JSON.stringify(wanted) +
          "}"
      );
    });
    row.appendChild(name);
    return row;
  }

  function zoneCard(zone, everyGroup) {
    var card = element("section", { class: "zone", "data-zone": zone.id });
    card.appendChild(element("h2", { "data-zone-name": zone.id }, zone.name));
    card.appendChild(
      element(
        "p",
        { class: "meta", "data-zone-meta": zone.id },
        "id " +
          zone.id +
          " · group " +
          zone.group +
          " · " +
          (zone.present.length === 1
            ? "1 endpoint playing"
            : zone.present.length + " endpoints playing") +
          " · stream " +
          zone.audio
      )
    );
    card.appendChild(volumeRow(zone));
    card.appendChild(muteRow(zone));
    card.appendChild(groupRow(zone, everyGroup));
    return card;
  }

  function render(state) {
    if (state.serial === lastSerial) {
      return;
    }
    lastSerial = state.serial;
    var loading = document.querySelector("[data-loading]");
    if (loading) {
      loading.remove();
    }
    zonesEl.textContent = "";
    if (!state.zones || state.zones.length === 0) {
      zonesEl.appendChild(emptyState());
    } else {
      var everyGroup = [];
      state.zones.forEach(function (zone) {
        [zone.group, zone.id].forEach(function (group) {
          if (everyGroup.indexOf(group) === -1) {
            everyGroup.push(group);
          }
        });
      });
      everyGroup.sort();
      state.zones.forEach(function (zone) {
        zonesEl.appendChild(zoneCard(zone, everyGroup));
      });
    }
    serialEl.textContent =
      "state " + state.serial + " · catalog version " + state.v;
  }

  function live() {
    var events = new EventSource("/api/events");
    events.onopen = function () {
      connectionEl.textContent = "live";
    };
    events.onmessage = function (event) {
      render(JSON.parse(event.data));
    };
    events.onerror = function () {
      // A dropped event stream is not a reason to show a stale page as live.
      // EventSource reconnects by itself; what changes here is what the page
      // admits to.
      connectionEl.textContent = "reconnecting";
    };
  }

  // The state is fetched once as well as subscribed to, so the page shows
  // something even where the event stream cannot be opened at all.
  fetch("/api/state")
    .then(function (response) {
      return response.json();
    })
    .then(render)
    .catch(function (e) {
      var loading = document.querySelector("[data-loading]");
      if (loading) {
        loading.remove();
      }
      zonesEl.appendChild(
        element(
          "p",
          { "data-error": "" },
          "This page could not reach the server it was served by: " + e
        )
      );
    })
    .finally(live);
})();
