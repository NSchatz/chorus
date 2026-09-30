#include "fake_amp.h"

#include <stdio.h>
#include <string.h>

static void record(fake_amp_t *fake, fake_event_kind_t kind, uint8_t address, uint8_t reg,
                   uint8_t value, uint32_t ms, chorus_i2c_result_t result)
{
    if (fake->event_count >= FAKE_MAX_EVENTS) {
        return;
    }
    fake_event_t *event = &fake->events[fake->event_count++];
    event->kind = kind;
    event->address = address;
    event->reg = reg;
    event->value = value;
    event->ms = ms;
    event->result = result;
}

/* The control port's reset values that matter here (pp. 48, 53, 63, 72, 73). */
static void reset_registers(fake_amp_t *fake)
{
    memset(fake->registers, 0, sizeof(fake->registers));
    fake->registers[FAKE_TAS_REG_DEVICE_CTRL2] = 0x10;
    fake->registers[FAKE_TAS_REG_SAP_CTRL1] = 0x02;
    fake->registers[FAKE_TAS_REG_DIE_ID] = fake->die_id;
    fake->book = 0;
    fake->page = 0;
    fake->dsp_enabled = 0;
    memset(fake->latched_fault, 0, sizeof(fake->latched_fault));
}

void fake_amp_init(fake_amp_t *fake)
{
    memset(fake, 0, sizeof(*fake));
    fake->address = FAKE_TAS_ADDRESS;
    fake->answer = CHORUS_I2C_ACK;
    fake->die_id = FAKE_TAS_DIE_ID;
    fake->clock_fault_without_clock = 1;
    fake->pdn = FAKE_PDN_UNKNOWN;
    reset_registers(fake);
}

static uint8_t fault_register(const fake_amp_t *fake, size_t i)
{
    uint8_t v = (uint8_t)(fake->persistent_fault[i] | fake->latched_fault[i]);
    return v;
}

static int any_fault(const fake_amp_t *fake)
{
    for (size_t i = 0; i < 3; i++) {
        if (fault_register(fake, i) != 0) {
            return 1;
        }
    }
    return 0;
}

static int ctrl_state(const fake_amp_t *fake)
{
    return fake->registers[FAKE_TAS_REG_DEVICE_CTRL2] & 0x03;
}

int fake_amp_switching(const fake_amp_t *fake)
{
    /* A board that never drove PDN is not known to be dead either. */
    if (fake->pdn == FAKE_PDN_UNKNOWN) {
        return 1;
    }
    return fake->pdn == FAKE_PDN_HIGH && ctrl_state(fake) == 3 && fake->dsp_enabled &&
           fake->clock_running && !fake->refuses_play && !any_fault(fake);
}

/* The part's own state machine, re-evaluated after anything changes. */
static void settle(fake_amp_t *fake)
{
    if (fake->pdn != FAKE_PDN_HIGH) {
        return;
    }
    if (!fake->clock_running && fake->clock_fault_without_clock) {
        fake->latched_fault[1] |= FAKE_TAS_CLK_FAULT;
    }
    int state = ctrl_state(fake);
    uint8_t reported = (uint8_t)state;
    if (state == 3 && (!fake->clock_running || fake->refuses_play || any_fault(fake))) {
        reported = 2; /* a halted clock or a fault: Hi-Z (p. 29) */
    }
    fake->registers[FAKE_TAS_REG_POWER_STATE] = reported;
}

static chorus_i2c_result_t transaction(fake_amp_t *fake, uint8_t address)
{
    if (fake->pdn != FAKE_PDN_HIGH) {
        fake->i2c_while_pdn_low = 1;
        return CHORUS_I2C_NACK;
    }
    if (fake->now_ms - fake->powered_at_ms < 5u) {
        fake->i2c_before_power_up_wait = 1;
    }
    return (address == fake->address) ? fake->answer : CHORUS_I2C_NACK;
}

static chorus_i2c_result_t fake_read(void *ctx, uint8_t address, uint8_t reg, uint8_t *value)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    settle(fake);
    chorus_i2c_result_t result = transaction(fake, address);
    uint8_t answered = 0;
    if (result == CHORUS_I2C_ACK) {
        if (reg >= FAKE_TAS_REG_CHAN_FAULT && reg <= FAKE_TAS_REG_GLOBAL_FAULT2) {
            answered = fault_register(fake, (size_t)(reg - FAKE_TAS_REG_CHAN_FAULT));
        } else if (fake->book == 0 && fake->page == 0) {
            answered = fake->registers[reg];
        }
        *value = answered;
    }
    record(fake, FAKE_EV_I2C_READ, address, reg, answered, 0, result);
    return result;
}

static chorus_i2c_result_t fake_write(void *ctx, uint8_t address, uint8_t reg, uint8_t value)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    chorus_i2c_result_t result = transaction(fake, address);
    record(fake, FAKE_EV_I2C_WRITE, address, reg, value, 0, result);
    if (result != CHORUS_I2C_ACK) {
        return result;
    }
    if (reg == FAKE_TAS_REG_PAGE) {
        fake->page = value;
        return result;
    }
    if (reg == FAKE_TAS_REG_BOOK && fake->page == 0) {
        fake->book = value;
        return result;
    }
    if (reg == FAKE_TAS_REG_FAULT_CLEAR) {
        if (value & 0x80) {
            memset(fake->latched_fault, 0, sizeof(fake->latched_fault));
        }
        settle(fake);
        return result;
    }
    fake->registers[reg] = value;
    if (reg == FAKE_TAS_REG_DEVICE_CTRL2) {
        int enabled = (value & 0x10) == 0 && (value & 0x03) >= 2;
        if (enabled && !fake->dsp_enabled) {
            fake->dsp_enabled_at_ms = fake->now_ms;
        }
        fake->dsp_enabled = enabled;
        if ((value & 0x03) == 3 && (!enabled || fake->now_ms - fake->dsp_enabled_at_ms < 5u)) {
            fake->play_before_dsp_settled = 1;
        }
    }
    settle(fake);
    return result;
}

static void note_clock_change(fake_amp_t *fake)
{
    fake->clock_changes++;
    if (fake_amp_switching(fake)) {
        fake->clock_changed_while_switching = 1;
    }
}

static int fake_high_impedance(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    if (fake->stage_refuses_high_impedance) {
        return -1;
    }
    fake->pdn = FAKE_PDN_LOW;
    record(fake, FAKE_EV_HIGH_IMPEDANCE, 0, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static int fake_power_up(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    if (fake->stage_refuses_power_up) {
        return -1;
    }
    if (fake->pdn != FAKE_PDN_HIGH) {
        fake->pdn = FAKE_PDN_HIGH;
        fake->powered_at_ms = fake->now_ms;
        reset_registers(fake);
        settle(fake);
    }
    record(fake, FAKE_EV_POWER_UP, 0, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static void fake_wait_ms(void *ctx, uint32_t ms)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    fake->now_ms += ms;
    record(fake, FAKE_EV_WAIT, 0, 0, 0, ms, CHORUS_I2C_ACK);
}

static int fake_apply_clock(void *ctx, const chorus_i2s_clock_t *clock)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    (void)clock;
    if (fake->controller_refuses_clock) {
        return -1;
    }
    note_clock_change(fake);
    fake->clock_running = 1;
    settle(fake);
    record(fake, FAKE_EV_CLOCK_APPLIED, 0, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static int fake_stop_clock(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    /* Stopping the clock IS a clock change: counting only apply_clock would
     * make the assertion blind to the teardown paths. */
    note_clock_change(fake);
    fake->clock_running = 0;
    settle(fake);
    record(fake, FAKE_EV_CLOCK_STOPPED, 0, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

chorus_i2c_bus_t fake_amp_bus(fake_amp_t *fake)
{
    chorus_i2c_bus_t bus;
    bus.ctx = fake;
    bus.read = fake_read;
    bus.write = fake_write;
    return bus;
}

chorus_output_stage_t fake_amp_stage(fake_amp_t *fake)
{
    chorus_output_stage_t stage;
    stage.ctx = fake;
    stage.high_impedance = fake_high_impedance;
    stage.power_up = fake_power_up;
    stage.wait_ms = fake_wait_ms;
    return stage;
}

chorus_i2s_controller_t fake_amp_controller(fake_amp_t *fake)
{
    chorus_i2s_controller_t controller;
    controller.ctx = fake;
    controller.apply_clock = fake_apply_clock;
    controller.stop_clock = fake_stop_clock;
    return controller;
}

int fake_amp_first(const fake_amp_t *fake, fake_event_kind_t kind)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind) {
            return (int)i;
        }
    }
    return -1;
}

int fake_amp_first_read(const fake_amp_t *fake, uint8_t reg)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == FAKE_EV_I2C_READ && fake->events[i].reg == reg) {
            return (int)i;
        }
    }
    return -1;
}

int fake_amp_first_write(const fake_amp_t *fake, uint8_t reg)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == FAKE_EV_I2C_WRITE && fake->events[i].reg == reg) {
            return (int)i;
        }
    }
    return -1;
}

int fake_amp_first_write_of(const fake_amp_t *fake, uint8_t reg, uint8_t value)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == FAKE_EV_I2C_WRITE && fake->events[i].reg == reg &&
            fake->events[i].value == value) {
            return (int)i;
        }
    }
    return -1;
}

size_t fake_amp_count(const fake_amp_t *fake, fake_event_kind_t kind)
{
    size_t count = 0;
    for (size_t i = 0; i < fake->event_count; i++) {
        if (fake->events[i].kind == kind) {
            count++;
        }
    }
    return count;
}

size_t fake_amp_i2c_transactions(const fake_amp_t *fake)
{
    return fake_amp_count(fake, FAKE_EV_I2C_READ) + fake_amp_count(fake, FAKE_EV_I2C_WRITE);
}

const char *fake_event_kind_name(fake_event_kind_t kind)
{
    switch (kind) {
    case FAKE_EV_I2C_READ:
        return "i2c-read";
    case FAKE_EV_I2C_WRITE:
        return "i2c-write";
    case FAKE_EV_HIGH_IMPEDANCE:
        return "pdn-low";
    case FAKE_EV_POWER_UP:
        return "pdn-high";
    case FAKE_EV_WAIT:
        return "wait";
    case FAKE_EV_CLOCK_APPLIED:
        return "i2s-clock-applied";
    case FAKE_EV_CLOCK_STOPPED:
        return "i2s-clock-stopped";
    }
    return "unknown";
}

void fake_amp_print(const fake_amp_t *fake)
{
    for (size_t i = 0; i < fake->event_count; i++) {
        const fake_event_t *event = &fake->events[i];
        if (event->kind == FAKE_EV_I2C_READ || event->kind == FAKE_EV_I2C_WRITE) {
            printf("     %zu %s address=0x%02x reg=0x%02x value=0x%02x -> %s\n", i,
                   fake_event_kind_name(event->kind), event->address, event->reg, event->value,
                   chorus_i2c_result_name(event->result));
        } else if (event->kind == FAKE_EV_WAIT) {
            printf("     %zu wait %u ms\n", i, (unsigned)event->ms);
        } else {
            printf("     %zu %s\n", i, fake_event_kind_name(event->kind));
        }
    }
}
