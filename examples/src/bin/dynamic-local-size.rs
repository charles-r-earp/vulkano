// Copyright (c) 2020 The vulkano developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

// This example demonstrates how to compute and load Compute Shader local size
// layout in runtime through specialization constants using Physical Device metadata.
//
// Workgroup parallelism capabilities are varying between GPUs and setting them
// properly is important to achieve maximal performance that particular device
// can provide.

use std::{fs::File, io::BufWriter, path::Path};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer},
    command_buffer::{
        allocator::StandardCommandBufferAllocator, AutoCommandBufferBuilder, CommandBufferUsage,
        CopyImageToBufferInfo,
    },
    descriptor_set::{
        allocator::StandardDescriptorSetAllocator, PersistentDescriptorSet, WriteDescriptorSet,
    },
    device::{
        physical::PhysicalDeviceType, Device, DeviceCreateInfo, DeviceExtensions, QueueCreateInfo,
    },
    format::Format,
    image::{view::ImageView, ImageDimensions, StorageImage},
    instance::{Instance, InstanceCreateInfo, InstanceExtensions},
    memory::allocator::StandardMemoryAllocator,
    pipeline::{ComputePipeline, Pipeline, PipelineBindPoint},
    sync::{self, GpuFuture},
    VulkanLibrary,
};

fn main() {
    let library = VulkanLibrary::new().unwrap();
    let instance = Instance::new(
        library,
        InstanceCreateInfo {
            enabled_extensions: InstanceExtensions {
                // This extension is required to obtain physical device metadata
                // about the device workgroup size limits
                khr_get_physical_device_properties2: true,

                ..InstanceExtensions::empty()
            },
            // Enable enumerating devices that use non-conformant vulkan implementations. (ex. MoltenVK)
            enumerate_portability: true,
            ..Default::default()
        },
    )
    .unwrap();

    let device_extensions = DeviceExtensions {
        ..DeviceExtensions::empty()
    };
    let (physical_device, queue_family_index) = instance
        .enumerate_physical_devices()
        .unwrap()
        .filter(|p| p.supported_extensions().contains(&device_extensions))
        .filter_map(|p| {
            p.queue_family_properties()
                .iter()
                .position(|q| q.queue_flags.compute)
                .map(|i| (p, i as u32))
        })
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4,
            _ => 5,
        })
        .unwrap();

    println!(
        "Using device: {} (type: {:?})",
        physical_device.properties().device_name,
        physical_device.properties().device_type
    );

    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            enabled_extensions: device_extensions,
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let queue = queues.next().unwrap();

    mod cs {
        vulkano_shaders::shader! {
            ty: "compute",
            src: "
                #version 450

                // We set local_size_x and local_size_y to be variable configurable
                // values through Specialization Constants. Values 1 and 2 define
                // constant_id (1 and 2 correspondingly) and default values of
                // the constants both. The `local_size_z = 1` here is an ordinary
                // built-in value of the local size in Z axis.
                //
                // Unfortunately current GLSL language capabilities doesn't let us
                // define exact names of the constants so we will have to use
                // anonymous constants instead. See below on how to provide their
                // values in run time.
                //
                // Please NOTE that the constant_id in local_size layout must be
                // positive values. Zero value lead to runtime failure on nVidia
                // devices due to a known bug in nVidia driver.
                layout(local_size_x_id = 1, local_size_y_id = 2, local_size_z = 1) in;

                // We can still define more constants in the Shader
                layout(constant_id = 0) const float red = 0.0;
                layout(constant_id = 3) const float green = 0.0;
                layout(constant_id = 4) const float blue = 0.0;

                layout(set = 0, binding = 0, rgba8) uniform writeonly image2D img;

                void main() {
                    // Colorful Mandelbrot fractal

                    vec2 norm_coordinates = (gl_GlobalInvocationID.xy + vec2(0.5)) / vec2(imageSize(img));
                    vec2 c = (norm_coordinates - vec2(0.5)) * 2.0 - vec2(1.0, 0.0);

                    vec2 z = vec2(0.0, 0.0);
                    float i;
                    for (i = 0.0; i < 1.0; i += 0.005) {
                        z = vec2(
                            z.x * z.x - z.y * z.y + c.x,
                            z.y * z.x + z.x * z.y + c.y
                        );

                        if (length(z) > 4.0) {
                            break;
                        }
                    }

                    vec4 to_write = vec4(vec3(red, green, blue) * i, 1.0);

                    imageStore(img, ivec2(gl_GlobalInvocationID.xy), to_write);
                }
            "
        }
    }

    let shader = cs::load(device.clone()).unwrap();

    // Fetching subgroup size from the Physical Device metadata to compute appropriate
    // Compute Shader local size properties.
    //
    // Most of the drivers provide this metadata, but some of the drivers don't.
    // In this case we can find appropriate value in this table: https://vulkan.gpuinfo.org/
    // or just use fallback constant for simplicity, but failure to set proper
    // local size can lead to significant performance penalty.
    let (local_size_x, local_size_y) = match device.physical_device().properties().subgroup_size {
        Some(subgroup_size) => {
            println!("Subgroup size is {}", subgroup_size);

            // Most of the subgroup values are divisors of 8
            (8, subgroup_size / 8)
        }
        None => {
            println!("This Vulkan driver doesn't provide physical device Subgroup information");

            // Using fallback constant
            (8, 8)
        }
    };

    println!(
        "Local size will be set to: ({}, {}, 1)",
        local_size_x, local_size_y
    );

    let spec_consts = cs::SpecializationConstants {
        red: 0.2,
        green: 0.5,
        blue: 1.0,
        constant_1: local_size_x, // specifying local size constants
        constant_2: local_size_y,
    };
    let pipeline = ComputePipeline::new(
        device.clone(),
        shader.entry_point("main").unwrap(),
        &spec_consts,
        None,
        |_| {},
    )
    .unwrap();

    let memory_allocator = StandardMemoryAllocator::new_default(device.clone());
    let descriptor_set_allocator = StandardDescriptorSetAllocator::new(device.clone());
    let command_buffer_allocator =
        StandardCommandBufferAllocator::new(device.clone(), Default::default());

    let image = StorageImage::new(
        &memory_allocator,
        ImageDimensions::Dim2d {
            width: 1024,
            height: 1024,
            array_layers: 1,
        },
        Format::R8G8B8A8_UNORM,
        Some(queue.queue_family_index()),
    )
    .unwrap();
    let view = ImageView::new_default(image.clone()).unwrap();

    let layout = pipeline.layout().set_layouts().get(0).unwrap();
    let set = PersistentDescriptorSet::new(
        &descriptor_set_allocator,
        layout.clone(),
        [WriteDescriptorSet::image_view(0, view)],
    )
    .unwrap();

    let buf = CpuAccessibleBuffer::from_iter(
        &memory_allocator,
        BufferUsage {
            transfer_dst: true,
            ..BufferUsage::empty()
        },
        false,
        (0..1024 * 1024 * 4).map(|_| 0u8),
    )
    .unwrap();

    let mut builder = AutoCommandBufferBuilder::primary(
        &command_buffer_allocator,
        queue.queue_family_index(),
        CommandBufferUsage::OneTimeSubmit,
    )
    .unwrap();
    builder
        .bind_pipeline_compute(pipeline.clone())
        .bind_descriptor_sets(
            PipelineBindPoint::Compute,
            pipeline.layout().clone(),
            0,
            set,
        )
        .dispatch([
            1024 / local_size_x, // Note that dispatch dimensions must be
            1024 / local_size_y, // proportional to local size
            1,
        ])
        .unwrap()
        .copy_image_to_buffer(CopyImageToBufferInfo::image_buffer(image, buf.clone()))
        .unwrap();
    let command_buffer = builder.build().unwrap();

    let future = sync::now(device)
        .then_execute(queue, command_buffer)
        .unwrap()
        .then_signal_fence_and_flush()
        .unwrap();

    future.wait(None).unwrap();

    println!("Success");

    let buffer_content = buf.read().unwrap();
    let path = Path::new("mandelbrot.png");
    let file = File::create(path).unwrap();
    let w = &mut BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, 1024, 1024);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&buffer_content).unwrap();
}
