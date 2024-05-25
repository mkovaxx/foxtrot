use clap::{App, Arg};
use gltf::json::{self as gltf_json, validation::USize64};
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
    let tree = convert_to_node_tree(&entities);
    let end = std::time::SystemTime::now();
    let since_the_epoch = end.duration_since(start).expect("Time went backwards");
    println!("Triangulated in {:?}", since_the_epoch);

    if let Some(out_path) = matches.value_of("output") {
        export(&out_path, tree);
    }

    Ok(())
}

fn export(path: &str, tree: triangulate::triangulate::NodeTree) {
    use crate::gltf_json::validation::Checked::Valid;

    let mut root = gltf_json::root::Root::default();

    let (min, max) = bounding_coords(&tree.vertices);

    let positions_count = tree.vertices.len();

    let positions_view_length = tree.vertices.len() * mem::size_of::<Vertex>();
    let indices_view_length = tree.triangles.len() * mem::size_of::<Triangle>();

    let mut buffer_data: Vec<u8> = vec![];
    buffer_data.append(&mut to_padded_byte_vector(tree.vertices));
    let indices_view_offset = buffer_data.len();
    buffer_data.append(&mut to_padded_byte_vector(tree.triangles));

    let buffer = root.push(gltf_json::Buffer {
        byte_length: USize64::from(buffer_data.len()),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        uri: None,
    });

    let positions_view = root.push(gltf_json::buffer::View {
        buffer,
        byte_length: USize64::from(positions_view_length),
        byte_offset: None,
        byte_stride: Some(gltf_json::buffer::Stride(mem::size_of::<Vertex>())),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        target: Some(Valid(gltf_json::buffer::Target::ArrayBuffer)),
    });

    let positions = root.push(gltf_json::Accessor {
        buffer_view: Some(positions_view),
        byte_offset: Some(USize64(0)),
        count: USize64::from(positions_count),
        component_type: Valid(gltf_json::accessor::GenericComponentType(
            gltf_json::accessor::ComponentType::F32,
        )),
        extensions: Default::default(),
        extras: Default::default(),
        type_: Valid(gltf_json::accessor::Type::Vec3),
        min: Some(gltf_json::Value::from(Vec::from(min))),
        max: Some(gltf_json::Value::from(Vec::from(max))),
        name: None,
        normalized: false,
        sparse: None,
    });

    let indices_view = root.push(gltf_json::buffer::View {
        buffer: buffer,
        byte_length: USize64::from(indices_view_length),
        byte_offset: Some(USize64::from(indices_view_offset)),
        byte_stride: Some(gltf_json::buffer::Stride(mem::size_of::<Triangle>())),
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        target: Some(Valid(gltf_json::buffer::Target::ArrayBuffer)),
    });

    // translate Nodes into glTF nodes
    for node in tree.nodes {
        let indices_offset = node.triangle_index as u64 * mem::size_of::<Triangle>() as u64;
        let indices = root.push(gltf_json::Accessor {
            buffer_view: Some(indices_view),
            byte_offset: Some(USize64(indices_offset)),
            count: USize64::from(3 * node.triangle_count as u64),
            component_type: Valid(gltf_json::accessor::GenericComponentType(
                gltf_json::accessor::ComponentType::U32,
            )),
            extensions: Default::default(),
            extras: Default::default(),
            type_: Valid(gltf_json::accessor::Type::Scalar),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
        });

        let primitive = gltf_json::mesh::Primitive {
            attributes: {
                let mut map = std::collections::BTreeMap::new();
                map.insert(Valid(gltf_json::mesh::Semantic::Positions), positions);
                map
            },
            extensions: Default::default(),
            extras: Default::default(),
            indices: Some(indices),
            material: None,
            mode: Valid(gltf_json::mesh::Mode::Triangles),
            targets: None,
        };

        let mesh = root.push(gltf_json::Mesh {
            extensions: Default::default(),
            extras: Default::default(),
            name: None,
            primitives: vec![primitive],
            weights: None,
        });

        let children = node
            .children
            .into_iter()
            .map(|child_idx| gltf_json::Index::<gltf_json::Node>::new(child_idx.0))
            .collect();

        let node = root.push(gltf_json::Node {
            mesh: Some(mesh),
            children: Some(children),
            ..Default::default()
        });
    }

    let json_string = gltf_json::serialize::to_string(&root).expect("Serialization error");

    let mut json_offset = json_string.len();

    align_to_multiple_of_four(&mut json_offset);

    // TODO: fix this
    let root_nodes = vec![gltf_json::Index::<gltf_json::Node>::new(0)];

    root.push(gltf_json::Scene {
        extensions: Default::default(),
        extras: Default::default(),
        name: None,
        nodes: root_nodes,
    });

    let glb = gltf::binary::Glb {
        header: gltf::binary::Header {
            magic: *b"glTF",
            version: 2,
            // N.B., the size of binary glTF file is limited to range of `u32`.
            length: (json_offset + positions_view_length)
                .try_into()
                .expect("file size exceeds binary glTF limit"),
        },
        bin: Some(Cow::Owned(buffer_data)),
        json: Cow::Owned(json_string.into_bytes()),
    };

    let writer = std::fs::File::create(path).expect("I/O error");

    glb.to_writer(writer).expect("glTF binary output error");
}

type Vertex = [f32; 3];
type Triangle = [u32; 3];

/// Calculate bounding coordinates of a list of vertices, used for the clipping distance of the model
fn bounding_coords(points: &[Vertex]) -> (Vertex, Vertex) {
    let mut min = [f32::MAX, f32::MAX, f32::MAX];
    let mut max = [f32::MIN, f32::MIN, f32::MIN];

    for p in points {
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
