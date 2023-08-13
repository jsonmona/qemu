/*
 * QEMU VMware-SVGA "chipset" with Vulkan powered 3D support.
 *
 * Copyright (c) 2023 Yeomin Yoon  <jsonmona@outlook.com>
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

#include "vmsvga-impl.h" // vmware-svga-vulkan-impl library

#include "qemu/osdep.h"
#include "qemu/module.h"
#include "qemu/units.h"
#include "qapi/error.h"
#include "qemu/log.h"
#include "hw/loader.h"
#include "trace.h"
#include "hw/pci/pci_device.h"
#include "hw/qdev-properties.h"
//#include "migration/vmstate.h"
#include "migration/qemu-file.h"
#include "migration/register.h"
#include "qom/object.h"
#include "ui/console.h"

#undef VERBOSE
#define HW_RECT_ACCEL
#define HW_FILL_ACCEL
#define HW_MOUSE_ACCEL

#include "vga_int.h"

#define TYPE_VMSVGA_VK "vmware-svga-vulkan"

OBJECT_DECLARE_SIMPLE_TYPE(pci_vmsvga_vk_state_s, VMSVGA_VK);

struct pci_vmsvga_vk_state_s {
    /*< private >*/
    PCIDevice parent_obj;
    /*< public >*/

    VGACommonState vga;
    vmsvga_vk_impl* impl;
    MemoryRegion io_bar;
    MemoryRegion fifo_ram;
    uint32_t* scanout_buffer;
    size_t scanout_buffer_len;
};

#define SVGA_MAGIC              0x900000UL
#define SVGA_MAKE_ID(ver)       (SVGA_MAGIC << 8 | (ver))
#define SVGA_ID_1               SVGA_MAKE_ID(1)
#define SVGA_ID_2               SVGA_MAKE_ID(2)

#define SVGA_ID                SVGA_ID_2
#define SVGA_IO_MUL            1
#define SVGA_FIFO_SIZE         0x10000
#define SVGA_PCI_DEVICE_ID     PCI_DEVICE_ID_VMWARE_SVGA2

static void vmsvga_vk_invalidate_display(void *opaque)
{
    struct pci_vmsvga_vk_state_s *s = opaque;

    if (vmsvga_vk_is_vga_mode(s->impl)) {
        s->vga.hw_ops->invalidate(&s->vga);
    } else {
        g_free(s->scanout_buffer);
        s->scanout_buffer = NULL;
        s->scanout_buffer_len = 0;
        vmsvga_vk_invalidate(s->impl);
    }
}

static void vmsvga_vk_update_display(void *opaque)
{
    struct pci_vmsvga_vk_state_s *s = opaque;

    if (vmsvga_vk_is_vga_mode(s->impl) != 0) {
        s->vga.hw_ops->gfx_update(&s->vga);
    } else {
        uint32_t width, height, stride;
        vmsvga_vk_output_info(s->impl, &width, &height, &stride);

        size_t scanout_len = (size_t) stride * (size_t) height;

        if (s->scanout_buffer_len != scanout_len) {
            g_free(s->scanout_buffer);
            s->scanout_buffer = g_malloc(scanout_len);

            if (s->scanout_buffer == NULL) {
                // Anything better to do?
                puts("FATAL: Unable to allocate scanout buffer!");
                fflush(stdout);
                abort();
            }

            s->scanout_buffer_len = scanout_len;
            memset(s->scanout_buffer, 0, scanout_len);
        }

        DisplaySurface *surface = qemu_console_surface(s->vga.con);

        if (width != surface_width(surface) || height != surface_height(surface)) {
            pixman_format_code_t format =
                qemu_default_pixman_format(32, true);
            //trace_vmware_setmode(width, height, 32);
            surface = qemu_create_displaysurface_from(width, height,
                                                    format, stride,
                                                    s->scanout_buffer);
            dpy_gfx_replace_surface(s->vga.con, surface);
        }

        // If fails, just use what's left in the buffer
        vmsvga_vk_output_read(s->impl, s->scanout_buffer, scanout_len);

        dpy_gfx_update_full(s->vga.con);
    }
}

static void vmsvga_vk_text_update(void *opaque, console_ch_t *chardata)
{
    struct pci_vmsvga_vk_state_s *s = opaque;

    if (s->vga.hw_ops->text_update) {
        s->vga.hw_ops->text_update(&s->vga, chardata);
    }
}

static const GraphicHwOps vmsvga_vk_ops = {
    .invalidate  = vmsvga_vk_invalidate_display,
    .gfx_update  = vmsvga_vk_update_display,
    .text_update = vmsvga_vk_text_update,
};

static void vmsvga_vk_init(DeviceState *dev, struct pci_vmsvga_vk_state_s *s,
                        MemoryRegion *address_space, MemoryRegion *io)
{
    s->vga.con = graphic_console_init(dev, 0, &vmsvga_vk_ops, s);
    s->vga.vram_size_mb = 32;  // You need to set this

    vga_common_init(&s->vga, OBJECT(dev), &error_fatal);
    vga_init(&s->vga, OBJECT(dev), address_space, io, true);

    ChipConfig config;
    vmsvga_vk_config_default(sizeof(ChipConfig), &config);

    config.fifo_len = 2*1024*1024;  // 2MiB
    memory_region_init_ram(&s->fifo_ram, NULL, "vmsvga-vk.fifo",
                           config.fifo_len, &error_fatal);
    config.fifo = memory_region_get_ram_ptr(&s->fifo_ram);

    config.fb = s->vga.vram_ptr;
    config.fb_len = s->vga.vram_size;

    s->impl = vmsvga_vk_new(&config);
}


static uint64_t vmsvga_vk_io_read(void *opaque, hwaddr addr, unsigned size)
{
    g_assert(size == 4);
    struct pci_vmsvga_vk_state_s *s = opaque;
    
    return vmsvga_vk_read_io4(s->impl, addr);
}

static void vmsvga_vk_io_write(void *opaque, hwaddr addr,
                            uint64_t data, unsigned size)
{
    g_assert(size == 4);
    struct pci_vmsvga_vk_state_s* s = opaque;

    vmsvga_vk_write_io4(s->impl, addr, data);
}

static const MemoryRegionOps vmsvga_vk_io_ops = {
    .read = vmsvga_vk_io_read,
    .write = vmsvga_vk_io_write,
    .endianness = DEVICE_LITTLE_ENDIAN,
    .valid = {
        .min_access_size = 4,
        .max_access_size = 4,
        .unaligned = true,
    },
    .impl = {
        .unaligned = true,
    },
};

static void pci_vmsvga_vk_realize(PCIDevice *dev, Error **errp)
{
    struct pci_vmsvga_vk_state_s *s = VMSVGA_VK(dev);

    dev->config[PCI_CACHE_LINE_SIZE] = 0x08;
    dev->config[PCI_LATENCY_TIMER] = 0x40;
    dev->config[PCI_INTERRUPT_LINE] = 0xff;          /* End */

    memory_region_init_io(&s->io_bar, OBJECT(dev), &vmsvga_vk_io_ops, s,
                          "vmsvga-vk-io", 0x10);
    memory_region_set_flush_coalesced(&s->io_bar);
    pci_register_bar(dev, 0, PCI_BASE_ADDRESS_SPACE_IO, &s->io_bar);

    vmsvga_vk_init(DEVICE(dev), s,
                pci_address_space(dev), pci_address_space_io(dev));

    pci_register_bar(dev, 1, PCI_BASE_ADDRESS_MEM_PREFETCH,
                     &s->vga.vram);
    pci_register_bar(dev, 2, PCI_BASE_ADDRESS_MEM_PREFETCH,
                     &s->fifo_ram);
}

static SaveVMHandlers savevm_vmsvga_vk_handlers = {
    .save_setup = NULL,
    .save_live_iterate = NULL,
    .save_live_complete_precopy = NULL,
    .state_pending_exact = NULL,
    .state_pending_estimate = NULL,
    .save_cleanup = NULL,
    .load_state = NULL,
    .is_active = NULL,
};

static void vmsvga_vk_instance_init(Object *obj)
{
    struct pci_vmsvga_vk_state_s *s = VMSVGA_VK(obj);
    register_savevm_live(TYPE_VMSVGA_VK, -1, 0,
                         &savevm_vmsvga_vk_handlers, s);
}

static void vmsvga_vk_instance_finalize(Object* obj)
{
    puts("finalize");
    struct pci_vmsvga_vk_state_s *s = VMSVGA_VK(obj);
    vmsvga_vk_freep(&s->impl);
}

static void vmsvga_vk_class_init(ObjectClass *klass, void *data)
{
    DeviceClass *dc = DEVICE_CLASS(klass);
    PCIDeviceClass *k = PCI_DEVICE_CLASS(klass);

    k->realize = pci_vmsvga_vk_realize;
    k->romfile = "vgabios-vmware.bin";
    k->vendor_id = PCI_VENDOR_ID_VMWARE;
    k->device_id = SVGA_PCI_DEVICE_ID;
    k->class_id = PCI_CLASS_DISPLAY_VGA;
    k->subsystem_vendor_id = PCI_VENDOR_ID_VMWARE;
    k->subsystem_id = SVGA_PCI_DEVICE_ID;
    dc->reset = NULL; //vmsvga_reset;
    dc->vmsd = NULL; //&vmstate_vmware_vga;
    //device_class_set_props(dc, vga_vmware_properties);
    dc->hotpluggable = false;
    set_bit(DEVICE_CATEGORY_DISPLAY, dc->categories);
}

static const TypeInfo vmsvga_vk_info = {
    .name          = TYPE_VMSVGA_VK,
    .parent        = TYPE_PCI_DEVICE,
    .instance_init = vmsvga_vk_instance_init,
    .instance_finalize = vmsvga_vk_instance_finalize,
    .instance_size = sizeof(struct pci_vmsvga_vk_state_s),
    .class_init    = vmsvga_vk_class_init,
    .interfaces = (InterfaceInfo[]) {
        { INTERFACE_CONVENTIONAL_PCI_DEVICE },
        { },
    },
};

static void vmsvga_vk_register_types(void)
{
    type_register_static(&vmsvga_vk_info);
}

type_init(vmsvga_vk_register_types)
