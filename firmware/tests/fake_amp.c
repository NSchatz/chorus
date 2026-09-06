#include "fake_amp.h"

#include <stdio.h>
#include <string.h>

static void record(fake_amp_t *fake, fake_event_kind_t kind, uint8_t address, uint8_t reg,
                   uint8_t value, chorus_i2c_result_t result)
{
    if (fake->event_count >= FAKE_MAX_EVENTS) {
        return;
    }
    fake_event_t *event = &fake->events[fake->event_count++];
    event->kind = kind;
    event->address = address;
    event->reg = reg;
    event->value = value;
    event->result = result;
}

void fake_amp_init(fake_amp_t *fake, uint8_t address)
{
    memset(fake, 0, sizeof(*fake));
    fake->address = address;
    fake->answer = CHORUS_I2C_ACK;
    /* A part powers up with its output stage in an UNKNOWN state, not a safe
     * one. Anything else would hand the sequencer the very property it is
     * supposed to establish. */
    fake->stage = FAKE_STAGE_UNKNOWN;
}

static chorus_i2c_result_t fake_read(void *ctx, uint8_t address, uint8_t reg, uint8_t *value)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    chorus_i2c_result_t result = (address == fake->address) ? fake->answer : CHORUS_I2C_NACK;
    uint8_t answered = (result == CHORUS_I2C_ACK) ? fake->registers[reg] : 0;
    record(fake, FAKE_EV_I2C_READ, address, reg, answered, result);
    if (result == CHORUS_I2C_ACK) {
        *value = answered;
    }
    return result;
}

static chorus_i2c_result_t fake_write(void *ctx, uint8_t address, uint8_t reg, uint8_t value)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    chorus_i2c_result_t result = (address == fake->address) ? fake->answer : CHORUS_I2C_NACK;
    record(fake, FAKE_EV_I2C_WRITE, address, reg, value, result);
    if (result == CHORUS_I2C_ACK) {
        fake->registers[reg] = value;
    }
    return result;
}

static int fake_high_impedance(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    if (fake->stage_refuses_high_impedance) {
        return -1;
    }
    fake->stage = FAKE_STAGE_HIGH_IMPEDANCE;
    record(fake, FAKE_EV_HIGH_IMPEDANCE, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static int fake_enable(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    if (fake->stage_refuses_enable) {
        return -1;
    }
    fake->stage = FAKE_STAGE_ENABLED;
    record(fake, FAKE_EV_OUTPUT_ENABLE, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static int fake_apply_clock(void *ctx, const chorus_i2s_clock_t *clock)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    (void)clock;
    if (fake->controller_refuses_clock) {
        return -1;
    }
    /* The assertion AC-2's second half is really about, made at the pin rather
     * than inferred from the order of a log: was the output stage dead when
     * the clock moved? */
    fake->clock_changes++;
    if (fake->stage != FAKE_STAGE_HIGH_IMPEDANCE) {
        fake->clock_changed_while_not_high_impedance = 1;
    }
    record(fake, FAKE_EV_CLOCK_APPLIED, 0, 0, 0, CHORUS_I2C_ACK);
    return 0;
}

static int fake_stop_clock(void *ctx)
{
    fake_amp_t *fake = (fake_amp_t *)ctx;
    /* Stopping the clock IS a clock change. Counting only apply_clock would
     * make the assertion above blind to the teardown paths, which are the ones
     * that run with the output stage already live. */
    fake->clock_changes++;
    if (fake->stage != FAKE_STAGE_HIGH_IMPEDANCE) {
        fake->clock_changed_while_not_high_impedance = 1;
    }
    record(fake, FAKE_EV_CLOCK_STOPPED, 0, 0, 0, CHORUS_I2C_ACK);
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
    stage.enable = fake_enable;
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
        return "high-impedance";
    case FAKE_EV_OUTPUT_ENABLE:
        return "output-enable";
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
        } else {
            printf("     %zu %s\n", i, fake_event_kind_name(event->kind));
        }
    }
}
