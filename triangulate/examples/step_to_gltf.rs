use clap::{App, Arg};
use gltf::json as gltf_json;
use std::{borrow::Cow, convert::TryInto, mem};

use step::step_file::StepFile;
use triangulate::triangulate::convert_to_node_tree;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = App::new("step_to_gltf")
        .author("Mate Kovacs <mkovaxx@gmail.com>")
        .about("Converts a STEP file to a glTF file")
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("out")
                .help("glTF file to target")
                .takes_value(true)
                .required(true),
        )
        .arg(Arg::with_name("input").takes_value(true).required(true))
        .get_matches();
    let input = matches.value_of("input").expect("Could not get input file");

    let start = std::time::SystemTime::now();
    let data = std::fs::read(input)?;
    let flat = StepFile::strip_flatten(&data);
    let entities = StepFile::parse(&flat);
    let end = std::time::SystemTime::now();
    let since_the_epoch = end.duration_since(start).expect("Time went backwards");
    println!("Loaded + parsed in {:?}", since_the_epoch);

    let start = std::time::SystemTime::now();
    let gltf = convert_to_node_tree(&entities);
    let end = std::time::SystemTime::now();
    let since_the_epoch = end.duration_since(start).expect("Time went backwards");
    println!("Triangulated in {:?}", since_the_epoch);

    if let Some(out_path) = matches.value_of("output") {
        export(&out_path, &gltf);
    }

    Ok(())
}

fn export(path: &str, root: &gltf_json::Root) {
    let triangle_vertices: Vec<Vertex> = vec![];

    let json_string = gltf_json::serialize::to_string(&root).expect("Serialization error");

    let mut json_offset = json_string.len();

    align_to_multiple_of_four(&mut json_offset);

    let buffer_length = triangle_vertices.len() * mem::size_of::<Vertex>();

    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            // N.B., the size of binary glTF file is limited to range of `u32`.
            length: (json_offset + buffer_length)
                .try_into()
                .expect("file size exceeds binary glTF limit"),
        },
        bin: Some(Cow::Owned(to_padded_byte_vector(triangle_vertices))),
        json: Cow::Owned(json_string.into_bytes()),
    };

    let writer = std::fs::File::create(path).expect("I/O error");

    glb.to_writer(writer).expect("glTF binary output error");
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
struct Vertex {
    position: [f32; 3],
}

/// Calculate bounding coordinates of a list of vertices, used for the clipping distance of the model
fn bounding_coords(points: &[Vertex]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for point in points {
        let p = point.position;
        for i in 0..3 {
            min[i] = f32::min(min[i], p[i]);
            max[i] = f32::max(max[i], p[i]);
        }
    }
    (min, max)
}

fn align_to_multiple_of_four(n: &mut usize) {
    *n = (*n + 3) & !3;
}

fn to_padded_byte_vector<T>(vec: Vec<T>) -> Vec<u8> {
    let byte_length = vec.len() * mem::size_of::<T>();
    let byte_capacity = vec.capacity() * mem::size_of::<T>();
    let alloc = vec.into_boxed_slice();
    let ptr = Box::<[T]>::into_raw(alloc) as *mut u8;
    let mut new_vec = unsafe { Vec::from_raw_parts(ptr, byte_length, byte_capacity) };
    while new_vec.len() % 4 != 0 {
        new_vec.push(0); // pad to multiple of four bytes
    }
    new_vec
}
