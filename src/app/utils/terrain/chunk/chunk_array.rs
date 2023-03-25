use crate::app::utils::terrain::chunk::commands;

use {
    crate::app::utils::{
        cfg, logger,
        terrain::{
            chunk::{
                EditError, Sides,
                prelude::*, Id,
                tasks::{FullTask, LowTask, Task, GenTask}
            },
            voxel::{Voxel, self},
        },
        saves::Save,
        reinterpreter::*,
        graphics::camera::Camera,
        concurrency::loading,
        user_io::{Key, keyboard, mouse},
    },
    math_linear::{prelude::*, math::ray::space_3d::Line},
    std::{
        slice::{Iter, IterMut},
        collections::{HashMap, HashSet},
        io, mem,
        sync::Mutex,
    },
    glium::{self as gl, backend::Facade},
    thiserror::Error,
    tokio::task::{JoinHandle, JoinError},
};

#[derive(Clone, Copy, Debug)]
enum ChunkArrSaveType {
    Sizes,
    Array,
}

impl From<ChunkArrSaveType> for u64 {
    fn from(value: ChunkArrSaveType) -> Self { value as u64 }
}

/// Represents 3d array of [`Chunk`]s. Can control their mesh generation, etc.
#[derive(Debug)]
pub struct ChunkArray {
    pub chunks: Vec<Chunk>,
    pub sizes: USize3,

    pub full_tasks: HashMap<Int3, FullTask>,
    pub low_tasks: HashMap<(Int3, Lod), LowTask>,
    pub voxels_gen_tasks: HashMap<Int3, GenTask>,

    pub lod_dist_threashold: f32,

    pub reading_handle: Option<JoinHandle<io::Result<(USize3, Vec<(Vec<Id>, FillType)>)>>>,
    pub saving_handle: Option<JoinHandle<io::Result<()>>>,
}

impl Default for ChunkArray {
    fn default() -> Self {
        Self {
            chunks: Default::default(),
            sizes: Default::default(),
            full_tasks: Default::default(),
            low_tasks: Default::default(),
            voxels_gen_tasks: Default::default(),
            lod_dist_threashold: 5.8,
            reading_handle: None,
            saving_handle: None,
        }
    }
}

impl ChunkArray {
    /// Generates new chunks.
    /// # Panic
    /// Panics if `sizes` is not valid. See `ChunkArray::validate_sizes()`.
    pub fn new(sizes: USize3) -> Self {
        Self::validate_sizes(sizes);
        let (start_pos, end_pos) = Self::pos_bounds(sizes);

        let chunks = SpaceIter::new(start_pos..end_pos)
            .map(Chunk::new)
            .collect();

        Self::from_chunks(sizes, chunks)
    }

    /// Constructs [`ChunkArray`] with passed in chunks.
    /// # Panic
    /// Panics if `sizes` is not valid. See `ChunkArray::validate_sizes()`.
    pub fn from_chunks(sizes: USize3, chunks: Vec<Chunk>) -> Self {
        Self::validate_sizes(sizes);
        let volume = Self::volume(sizes);
        assert_eq!(
            chunks.len(), volume,
            "passed in chunk `Vec` should have same size as passed in sizes, but sizes: {sizes}, len: {len}",
            len = chunks.len(),
        );
        
        Self { chunks, sizes, ..Default::default() }
    }

    /// Constructs [`ChunkArray`] with empty chunks.
    /// # Panic
    /// Panics if `sizes` is not valid. See `ChunkArray::validate_sizes()`.
    pub fn new_empty_chunks(sizes: USize3) -> Self {
        Self::validate_sizes(sizes);
        let (start_pos, end_pos) = Self::pos_bounds(sizes);

        let chunks = SpaceIter::new(start_pos..end_pos)
            .map(Chunk::new_empty)
            .collect();

        Self::from_chunks(sizes, chunks)
    }

    /// Computes start and end poses from chunk array sizes.
    pub fn pos_bounds(sizes: USize3) -> (Int3, Int3) {
        (
            Self::coord_idx_to_pos(sizes, USize3::ZERO),
            Self::coord_idx_to_pos(sizes, sizes),
        )
    }

    /// Checks that sizes is valid.
    /// # Panic
    /// Panics if `sizes.x * sizes.y * sizes.z` > `MAX_CHUNKS`.
    pub fn validate_sizes(sizes: USize3) {
        assert!(
            Self::volume(sizes) <= cfg::terrain::MAX_CHUNKS,
            "cannot allocate too many chunks: {volume}",
            volume = Self::volume(sizes),
        );
    }

    /// Gives empty [`ChunkArray`].
    pub fn new_empty() -> Self {
        Self::default()
    }

    pub async fn save_to_file(
        sizes: USize3, chunks: Vec<ChunkRef<'static>>, save_name: impl Into<String>, save_path: &'static str,
    ) -> io::Result<()> {
        let save_name = save_name.into();

        let _work_guard = logger::work("chunk array", format!("saving to {save_name} in {save_path}"));

        let is_all_generated = chunks.iter()
            .all(ChunkRef::is_generated);
        assert!(is_all_generated, "Chunks should be generated to save them to file");

        let volume = Self::volume(sizes);
        assert_eq!(volume, chunks.len(), "chunks should have same length as sizes volume");

        let loading = loading::start_new("Chunks saving");

        Save::new(save_name.clone())
            .create(save_path).await?
            .write(&sizes, ChunkArrSaveType::Sizes).await
            .pointer_array(volume, ChunkArrSaveType::Array, |i| {
                let chunks = &chunks;
                let loading = &loading;

                async move {
                    loading.refresh(i as f32 / (volume - 1) as f32);
                    Self::chunk_as_bytes(chunks[i])
                }
            }).await
            .save()
            .await?;

        Ok(())
    }

    pub async fn read_from_file(
        save_name: &str, save_path: &str,
    ) -> io::Result<(USize3, Vec<(Vec<Id>, FillType)>)> {
        let _work_guard = logger::work("chunk array", format!("reading chunks from {save_name} in {save_path}"));

        let loading = loading::start_new("Chunks reading");

        let mut save = Save::new(save_name)
            .open(save_path)
            .await?;
        
        let sizes = save.read(ChunkArrSaveType::Sizes).await;

        let chunks = save.read_pointer_array(ChunkArrSaveType::Array, |i, bytes| {
            let loading = &loading;

            async move {
                loading.refresh(i as f32 / (Self::volume(sizes) - 1) as f32);
                Self::array_filltype_from_bytes(&bytes)
            }
        }).await;

        Ok((sizes, chunks))
    }

    /// Reinterprets [chunk][Chunk] as bytes. It uses Huffman's compresstion.
    pub fn chunk_as_bytes(chunk: ChunkRef<'_>) -> Vec<u8> {
        use { std::iter::FromIterator, bit_vec::BitVec, huffman_compress as hc };

        match chunk.info.fill_type {
            FillType::AllSame(id) =>
                FillType::AllSame(id).as_bytes(),

            FillType::Default => {
                let n_voxels = chunk.voxel_ids.len();
                assert_eq!(
                    n_voxels, Chunk::VOLUME,
                    "cannot save unknown-sized chunk with size {n_voxels}",
                );

                let freqs = Self::count_voxel_frequencies(chunk.voxel_ids.iter().copied());
                let (book, _) = hc::CodeBuilder::from_iter(
                    freqs.iter().map(|(&k, &v)| (k, v))
                ).finish();
                let mut bits = BitVec::new();

                for &voxel_id in chunk.voxel_ids.iter() {
                    book.encode(&mut bits, &voxel_id)
                        .expect("voxel id should be in the book");
                }

                compose! {
                    FillType::Default.as_bytes(),
                    freqs.as_bytes(),
                    bits.as_bytes(),
                }.collect()
            }
        }
    }

    /// Reinterprets bytes as [chunk][Chunk] and reads [id][Id] array and [fill type][FillType] from it.
    pub fn array_filltype_from_bytes(bytes: &[u8]) -> (Vec<Id>, FillType) {
        use { std::iter::FromIterator, bit_vec::BitVec, huffman_compress as hc };

        let mut reader = ByteReader::new(bytes);
        let fill_type: FillType = reader.read()
            .expect("failed to reinterpret bytes");

        match fill_type {
            FillType::Default => {
                let freqs: HashMap<Id, usize> = reader.read()
                    .expect("failed to read frequencies map from bytes");

                let bits: BitVec = reader.read()
                    .expect("failed to read `BitVec` from bytes");

                let (_, tree) = hc::CodeBuilder::from_iter(freqs).finish();
                let voxel_ids: Vec<Id> = tree.unbounded_decoder(bits).collect();

                let is_id_valid = voxel_ids.iter()
                    .copied()
                    .all(voxel::is_id_valid);

                assert!(is_id_valid, "Voxel ids in voxel array should be valid");
                assert_eq!(voxel_ids.len(), Chunk::VOLUME, "There's should be Chunk::VOLUME voxels");

                (voxel_ids, FillType::Default)
            },

            FillType::AllSame(id) =>
                (vec![], FillType::AllSame(id)),
        }
    }

    /// Sets voxel's id with position `pos` to `new_id` and returns old [`Id`]. If voxel is 
    /// set then this function should drop all its meshes and the neighbor ones.
    /// # Error
    /// Returns [`Err`] if `new_id` is not valid or `pos` is not in this [chunk array][ChunkArray].
    pub fn set_voxel(&mut self, pos: Int3, new_id: Id) -> Result<Id, EditError> {
        let chunk_pos = Chunk::local_pos(pos);
        let chunk_idx = Self::pos_to_idx(self.sizes, chunk_pos)
            .ok_or(EditError::PosIdConversion(pos))?;

        // We know that `chunk_idx` is valid so we can get-by-index.
        let old_id = self.chunks[chunk_idx].set_voxel(pos, new_id)?;

        Ok(old_id)
    }

    /// Gives voxel if it is in the [array][ChunkArray].
    pub fn get_voxel(&self, pos: Int3) -> Option<Voxel> {
        let chunk_pos = Chunk::local_pos(pos);
        let chunk_idx = Self::pos_to_idx(self.sizes, chunk_pos)?;

        match self.chunks[chunk_idx].get_voxel_global(pos) {
            ChunkOption::Item(voxel) => Some(voxel),
            ChunkOption::OutsideChunk => unreachable!("pos {} is indeed in that chunk", pos),
        }
    }

    /// Fills volume of voxels to same [id][Id] and returnes `is_changed`.
    pub fn fill_voxels(&mut self, pos_from: Int3, pos_to: Int3, new_id: Id) -> Result<bool, EditError> {
        let chunk_pos_from = Chunk::local_pos(pos_from);
        let chunk_pos_to   = Chunk::local_pos(pos_to + Int3::from(Chunk::SIZES) - Int3::ONE);

        Self::pos_to_idx(self.sizes, chunk_pos_from)
            .ok_or(EditError::PosIdConversion(chunk_pos_from))?;

        Self::pos_to_idx(self.sizes, chunk_pos_to - Int3::ONE)
            .ok_or(EditError::PosIdConversion(chunk_pos_to - Int3::ONE))?;

        let mut is_changed = false;

        for chunk_pos in SpaceIter::new(chunk_pos_from..chunk_pos_to) {
            let idx = Self::pos_to_idx(self.sizes, chunk_pos)
                .expect("chunk_pos already valid");

            let min_voxel_pos = Chunk::global_pos(chunk_pos);
            let end_voxel_pos = min_voxel_pos + Int3::from(Chunk::SIZES);

            let pos_from = Int3::new(
                Ord::max(pos_from.x, min_voxel_pos.x),
                Ord::max(pos_from.y, min_voxel_pos.y),
                Ord::max(pos_from.z, min_voxel_pos.z),
            );

            let pos_to = Int3::new(
                Ord::min(pos_to.x, end_voxel_pos.x),
                Ord::min(pos_to.y, end_voxel_pos.y),
                Ord::min(pos_to.z, end_voxel_pos.z),
            );

            let chunk_changed = self.chunks[idx].fill_voxels(pos_from, pos_to, new_id)?;
            if chunk_changed {
                is_changed = true;
                
                for idx in Self::get_adj_chunks_idxs(self.sizes, chunk_pos).as_array() {
                    if let Some(idx) = idx {
                        self.chunks[idx].drop_all_meshes();
                    }
                }
            }
        }

        Ok(is_changed)
    }

    /// Drops all meshes from each [chunk][Chunk].
    pub fn drop_all_meshes(&mut self) {
        for chunk in self.chunks.iter_mut() {
            chunk.drop_all_meshes();
        }
    }

    fn count_voxel_frequencies(voxel_ids: impl IntoIterator<Item = Id>) -> HashMap<Id, usize> {
        let mut result = HashMap::new();

        for id in voxel_ids.into_iter() {
            match result.get_mut(&id) {
                None => drop(result.insert(id, 1)),
                Some(freq) => *freq += 1,
            }
        }

        result
    }

    // FIXME: make unmodifiable flag on chunks.
    pub fn make_static_refs(&self) -> Vec<ChunkRef<'static>>
    where
        Self: 'static,
    {
        self.chunks()
            .map(|chunk| unsafe { chunk.make_ref().as_static() })
            .collect()
    }

    pub fn apply_new(&mut self, sizes: USize3, chunk_arr: Vec<(Vec<Id>, FillType)>) {
        assert_eq!(Self::volume(sizes), chunk_arr.len(), "chunk array should have same len as sizes");

        self.drop_tasks();

        self.sizes = sizes;
        self.chunks = chunk_arr.into_iter()
            .enumerate()
            .map(|(idx, (voxel_ids, fill_type))| {
                let chunk_pos = Self::idx_to_pos(idx, sizes);
                match fill_type {
                    FillType::Default =>
                        Chunk::from_voxels(voxel_ids, chunk_pos),
                    FillType::AllSame(id) =>
                        Chunk::new_same_filled(chunk_pos, id),
                }
            })
            .collect();
    }

    /// Gives chunk count.
    pub fn volume(arr_sizes: USize3) -> usize {
        arr_sizes.x * arr_sizes.y * arr_sizes.z
    }

    /// Convertes 3d index into chunk pos.
    pub fn coord_idx_to_pos(sizes: USize3, coord_idx: USize3) -> Int3 {
        Int3::from(coord_idx) - Int3::from(sizes) / 2
    }

    /// Convertes chunk pos to 3d index.
    pub fn pos_to_coord_idx(sizes: USize3, pos: Int3) -> Option<USize3> {
        let sizes = Int3::from(sizes);
        let shifted = pos + sizes / 2;

        match 0 <= shifted.x && shifted.x < sizes.x &&
              0 <= shifted.y && shifted.y < sizes.y &&
              0 <= shifted.z && shifted.z < sizes.z
        {
            true  => Some(shifted.into()),
            false => None
        }
    }

    /// Convertes 3d index to an array index.
    pub fn coord_idx_to_idx(sizes: USize3, coord_idx: USize3) -> usize {
        sdex::get_index(&coord_idx.as_array(), &sizes.as_array())
    }

    /// Convertes [chunk][Chunk] pos to an array index.
    pub fn pos_to_idx(sizes: USize3, pos: Int3) -> Option<usize> {
        let coord_idx = Self::pos_to_coord_idx(sizes, pos)?;
        Some(Self::coord_idx_to_idx(sizes, coord_idx))
    }

    /// Convertes array index to 3d index.
    pub fn idx_to_coord_idx(idx: usize, sizes: USize3) -> USize3 {
        iterator::idx_to_coord_idx(idx, sizes)
    }

    /// Converts array index to chunk pos.
    pub fn idx_to_pos(idx: usize, sizes: USize3) -> Int3 {
        let coord_idx = Self::idx_to_coord_idx(idx, sizes);
        Self::coord_idx_to_pos(sizes, coord_idx)
    }

    /// Gives reference to chunk by its position.
    pub fn get_chunk_by_pos(&self, pos: Int3) -> Option<ChunkRef<'_>> {
        Self::get_chunk_by_pos_inner(&self.chunks, self.sizes, pos)
    }

    fn get_chunk_by_pos_inner(chunks: &[Chunk], sizes: USize3, pos: Int3) -> Option<ChunkRef<'_>> {
        let idx = Self::pos_to_idx(sizes, pos)?;
        Some(chunks[idx].make_ref())
    }

    /// Gives reference to chunk by its position.
    pub fn get_chunk_mut_by_pos(&mut self, pos: Int3) -> Option<&mut Chunk> {
        let idx = Self::pos_to_idx(self.sizes, pos)?;
        Some(&mut self.chunks[idx])
    }

    /// Gives adjacent chunks references by center chunk position.
    pub fn get_adj_chunks(&self, pos: Int3) -> ChunkAdj<'_> {
        Self::get_adj_chunks_inner(&self.chunks, self.sizes, pos)
    }

    /// Gives adjacent chunks references by center chunk position.
    fn get_adj_chunks_inner(chunks: &[Chunk], sizes: USize3, pos: Int3) -> ChunkAdj<'_> {
        let sides = Self::get_adj_chunks_idxs(sizes, pos)
            .map(|opt| opt.map(|idx|
                chunks[idx].make_ref()
            ));

        ChunkAdj { sides }
    }

    /// Gives '`iterator`' over adjacent to `pos` array indices.
    pub fn get_adj_chunks_idxs(sizes: USize3, pos: Int3) -> Sides<Option<usize>> {
        SpaceIter::adj_iter(pos)
            .map(|pos| Self::pos_to_idx(sizes, pos))
            .collect()
    }

    /// Gives iterator over chunk coordinates.
    pub fn pos_iter(sizes: USize3) -> SpaceIter {
        let (start, end) = Self::pos_bounds(sizes);
        SpaceIter::new(start..end)
    }

    /// Gives iterator over all chunk's adjacents.
    pub fn adj_iter(&self) -> impl Iterator<Item = ChunkAdj<'_>> {
        Self::adj_iter_inner(&self.chunks, self.sizes)
    }

    /// Gives iterator over all chunk's adjacents.
    fn adj_iter_inner(chunks: &[Chunk], sizes: USize3) -> impl Iterator<Item = ChunkAdj<'_>> {
        Self::pos_iter(sizes)
            .map(move |pos| Self::get_adj_chunks_inner(chunks, sizes, pos))
    }

    /// Gives iterator over desired LOD for each chunk.
    pub fn desired_lod_iter(chunk_array_sizes: USize3, cam_pos: vec3, threashold: f32) -> impl Iterator<Item = Lod> {
        Self::pos_iter(chunk_array_sizes)
            .map(move |chunk_pos| {
                let chunk_size = Chunk::GLOBAL_SIZE;
                let cam_pos_in_chunks = cam_pos / chunk_size;
                let chunk_pos = vec3::from(chunk_pos);

                let dist = (chunk_pos - cam_pos_in_chunks + vec3::all(0.5)).len();
                Lod::min(
                    (dist / threashold).floor() as Lod,
                    Chunk::SIZE.ilog2() as Lod,
                )
            })
    }

    /// Gives iterator over chunks.
    pub fn chunks(&self) -> Iter<'_, Chunk> {
        self.chunks.iter()
    }

    /// Gives mutable iterator over chunks.
    pub fn chunks_mut(&mut self) -> IterMut<'_, Chunk> {
        self.chunks.iter_mut()
    }

    /// Gives mutable iterator over chunks through shared reference.
    /// # Safety
    /// Following rust's aliasing rules, resulting mutable reference must
    /// be not aliased by others references.
    unsafe fn chunks_mut_shared(chunks: &[Chunk]) -> IterMut<'_, Chunk> {
        let chunks = (chunks as *const [Chunk] as *mut [Chunk])
            .as_mut()
            .unwrap_unchecked();

        chunks.iter_mut()
    }

    /// Gives iterator over all voxels in [`ChunkArray`].
    pub fn voxels(&self) -> impl Iterator<Item = Voxel> + '_ {
        self.chunks().flat_map(|chunk| chunk.voxels())
    }

    /// Gives iterator over mutable chunks and their adjacents.
    pub fn chunks_with_adj_mut(&mut self) -> impl Iterator<Item = (&mut Chunk, ChunkAdj<'_>)> + '_ {
        Self::chunks_with_adj_mut_inner(&mut self.chunks, self.sizes)
    }

    /// Gives iterator over mutable chunks and their adjacents.
    pub fn chunks_with_adj_mut_inner<'a>(
        chunks: &mut [Chunk], sizes: USize3,
    ) -> impl Iterator<Item = (&'a mut Chunk, ChunkAdj<'a>)> + 'a {
        let chunks = unsafe { (chunks as *const [Chunk]).as_ref().unwrap_unchecked() };

        // * Safe bacause shared adjacent chunks are not aliasing current mutable chunk.
        unsafe {
            Self::chunks_mut_shared(chunks)
                .zip(Self::adj_iter_inner(chunks, sizes))
        }
    }

    /// Generates mesh for each chunk.
    pub fn generate_meshes(&mut self, lod: impl Fn(Int3) -> Lod, facade: &dyn gl::backend::Facade) {
        for (chunk, adj) in self.chunks_with_adj_mut() {
            let active_lod = lod(chunk.pos);
            chunk.generate_mesh(active_lod, adj, facade);
            chunk.set_active_lod(active_lod);
        }
    }

    fn get_targets_sorted<'r>(
        chunks: &mut [Chunk], sizes: USize3, cam_pos: vec3, lod_threashold: f32,
    ) -> Vec<(&'r mut Chunk, ChunkAdj<'r>, u32)> {
        let mut result: Vec<_> = Self::chunks_with_adj_mut_inner(chunks, sizes)
            .zip(Self::desired_lod_iter(sizes, cam_pos, lod_threashold))
            .map(|((a, b), c)| (a, b, c))
            .collect();

        result.sort_by(|(lhs, _, _), (rhs, _, _)| {
            let l_pos = Chunk::global_pos(lhs.pos);
            let r_pos = Chunk::global_pos(rhs.pos);

            let l_dist = vec3::sqr(cam_pos - l_pos.into());
            let r_dist = vec3::sqr(cam_pos - r_pos.into());

            l_dist.partial_cmp(&r_dist)
                .expect("distance to chunk should be a number")
        });

        result
    }

    /// Renders all chunks. If chunk should have another LOD then it will start async
    /// task that generates desired mesh. If task is incomplete then it will render active LOD
    /// of concrete chunk. If it can't then it will do nothing.
    pub async fn render(
        &mut self, target: &mut impl gl::Surface, draw_bundle: &ChunkDrawBundle<'_>,
        uniforms: &impl gl::uniforms::Uniforms, facade: &dyn gl::backend::Facade, cam: &Camera,
    ) -> Result<(), ChunkRenderError>
    where
        Self: 'static,
    {
        let sizes = self.sizes;
        if sizes == USize3::ZERO { return Ok(()) }

        self.try_finish_all_tasks(facade).await;

        let chunks = Self::get_targets_sorted(&mut self.chunks, sizes, cam.pos, self.lod_dist_threashold);

        for (chunk, chunk_adj, lod) in chunks {
            if !chunk.is_generated() {
                if Self::is_voxels_gen_task_running(&self.voxels_gen_tasks, chunk.pos) {
                    Self::try_finish_voxels_gen_task(&mut self.voxels_gen_tasks, chunk.pos, chunk).await
                }
                
                else if self.can_start_tasks() {
                    Self::start_task_gen_voxels(&mut self.voxels_gen_tasks, chunk.pos);
                    continue;
                }

                else {
                    continue;
                }
            }

            let can_set_new_lod =
                chunk.get_available_lods().contains(&lod) ||
                Self::is_mesh_task_running(&self.full_tasks, &self.low_tasks, chunk.pos, lod) &&
                Self::try_finish_mesh_task(&mut self.full_tasks, &mut self.low_tasks,
                    chunk.pos, lod, chunk, facade).await.is_ok();

            if can_set_new_lod {
                chunk.set_active_lod(lod)
            }
            
            else if self.can_start_tasks() {
                // * Safety:
                // * Safe, because this function borrows chunk to mutate its meshes,
                // * but later it borrows to set new LOD value, so mut references
                // * are not aliasing. We can make reference `'static` due to
                // * `Self`'s lifetime is `'static`.
                unsafe {
                    Self::start_task_gen_vertices(
                        &mut self.full_tasks,
                        &mut self.low_tasks,
                        chunk.make_ref().as_static(),
                        chunk_adj,
                        lod,
                    );
                }
            }

            Self::drop_all_useless_tasks(&mut self.full_tasks, &mut self.low_tasks, lod, chunk.pos);

            if !chunk.can_render_active_lod() {
                chunk.try_set_best_fit_lod(lod);
            }

            // FIXME: make cam vis-check for light.
            if chunk.can_render_active_lod() && chunk.is_visible_by_camera(cam) {
                chunk.render(target, &draw_bundle, uniforms, chunk.info.active_lod)?
            }
        }

        Ok(())
    }

    pub fn drop_all_useless_tasks(
        full_tasks: &mut HashMap<Int3, FullTask>,
        low_tasks: &mut HashMap<(Int3, Lod), LowTask>,
        useful_lod: Lod, cur_pos: Int3,
    ) {
        for lod in Chunk::get_possible_lods() {
            if 2 < lod.abs_diff(useful_lod) {
                Self::drop_task(full_tasks, low_tasks, cur_pos, lod);
            }
        }
    }

    pub fn drop_task(
        full_tasks: &mut HashMap<Int3, FullTask>,
        low_tasks: &mut HashMap<(Int3, Lod), LowTask>,
        pos: Int3, lod: Lod,
    ) {
        match lod {
            0 =>   drop(full_tasks.remove(&pos)),
            lod => drop(low_tasks.remove(&(pos, lod))),
        }
    }

    pub async fn try_finish_full_tasks(&mut self, facade: &dyn gl::backend::Facade) {
        let full: Vec<_> = self.full_tasks.iter_mut()
            .filter(|(_, task)| match task.handle.as_ref() {
                None => false,
                Some(handle) => handle.is_finished()
            })
            .map(|(&pos, task)|
                (pos, task.take_result())
            )
            .collect();

        let mut new_full = Vec::with_capacity(full.len());
        for (pos, fut) in full {
            new_full.push((pos, fut.await));
        }

        for (pos, vertices) in new_full {
            self.full_tasks.remove(&pos);

            self.get_chunk_mut_by_pos(pos)
                .expect("pos should be valid")
                .upload_full_detail_vertices(&vertices, facade);
        }
    }

    pub async fn try_finish_low_tasks(&mut self, facade: &dyn gl::backend::Facade) {
        let low: Vec<_> = self.low_tasks.iter_mut()
            .filter(|(_, task)| match task.handle.as_ref() {
                None => false,
                Some(handle) => handle.is_finished()
            })
            .map(|(&(pos, lod), task)|
                (pos, lod, task.take_result())
            )
            .collect();

        let mut new_low = Vec::with_capacity(low.len());
        for (pos, lod, fut) in low {
            new_low.push((pos, lod, fut.await));
        }

        for (pos, lod, vertices) in new_low {
            self.low_tasks.remove(&(pos, lod));

            self.get_chunk_mut_by_pos(pos)
                .expect("pos should be valid")
                .upload_low_detail_vertices(&vertices, lod, facade);
        }
    }

    pub async fn try_finish_gen_tasks(&mut self) {
        let voxel_futs: Vec<_> = self.voxels_gen_tasks.iter_mut()
            .filter(|(_, task)| match task.handle {
                None => false,
                Some(ref handle) => handle.is_finished(),
            })
            .map(|(&pos, task)| (pos, task.take_result()))
            .collect();

        let mut voxel_vecs = Vec::with_capacity(voxel_futs.len());
        for (pos, fut) in voxel_futs {
            voxel_vecs.push((pos, fut.await));
        }

        for (pos, voxels) in voxel_vecs {
            self.voxels_gen_tasks.remove(&pos);

            let chunk = self.get_chunk_mut_by_pos(pos)
                .expect("pos should be valid");
            *chunk = Chunk::from_voxels(voxels, pos);
        }
    }

    pub async fn try_finish_all_tasks(&mut self, facade: &dyn gl::backend::Facade) {
        self.try_finish_full_tasks(facade).await;
        self.try_finish_low_tasks(facade).await;
        self.try_finish_gen_tasks().await;
    }

    pub fn is_voxels_gen_task_running(tasks: &HashMap<Int3, GenTask>, pos: Int3) -> bool {
        tasks.contains_key(&pos)
    }

    /// Checks if generate mesh task id running.
    pub fn is_mesh_task_running(
        full_tasks: &HashMap<Int3, FullTask>,
        low_tasks: &HashMap<(Int3, Lod), LowTask>,
        pos: Int3, lod: Lod
    ) -> bool {
        match lod {
            0 =>
                full_tasks.contains_key(&pos),
            lod =>
                low_tasks.contains_key(&(pos, lod)),
        }
    }

    pub fn start_task_gen_voxels(tasks: &mut HashMap<Int3, GenTask>, pos: Int3) {
        let prev_value = tasks.insert(pos, Task::spawn(async move {
            Chunk::generate_voxels(pos)
        }));

        assert!(prev_value.is_none(), "threre should be only one task");
    }

    /// Starts new generate vertices task.
    pub fn start_task_gen_vertices(
        full_tasks: &mut HashMap<Int3, FullTask>,
        low_tasks: &mut HashMap<(Int3, Lod), LowTask>,
        chunk: ChunkRef<'static>, adj: ChunkAdj<'static>, lod: Lod,
    ) {
        let pos = chunk.pos.to_owned();

        let is_adj_generated = adj.sides.inner
            .iter()
            .copied()
            .filter_map(std::convert::identity)
            .all(|chunk| chunk.is_generated());

        if !chunk.is_generated() || !is_adj_generated { return }

        match lod {
            0 => if !full_tasks.contains_key(&pos) {
                let prev = full_tasks.insert(pos, Task::spawn(async move {
                    chunk.make_vertices_detailed(adj)
                }));
                assert!(prev.is_none(), "there should be only one task");
            },

            lod if !low_tasks.contains_key(&(pos, lod)) => {
                let prev = low_tasks.insert((pos, lod), Task::spawn(async move {
                    chunk.make_vertices_low(adj, lod)
                }));
                assert!(prev.is_none(), "there should be only one task");
            },

            _ => (),
        }
    }

    pub async fn try_finish_voxels_gen_task(tasks: &mut HashMap<Int3, GenTask>, pos: Int3, chunk: &mut Chunk) {
        if let Some(task) = tasks.get_mut(&pos) {
            if let Some(voxel_ids) = task.try_take_result().await {
                *chunk = Chunk::from_voxels(voxel_ids, pos);
                tasks.remove(&pos);
            }
        }
    }

    /// Tries to get mesh from task if it is ready then sets it to chunk.
    /// Otherwise will return `Err(TaskError)`.
    pub async fn try_finish_mesh_task(
        full_tasks: &mut HashMap<Int3, FullTask>,
        low_tasks: &mut HashMap<(Int3, Lod), LowTask>,
        pos: Int3, lod: Lod,
        chunk: &mut Chunk, facade: &dyn gl::backend::Facade,
    ) -> Result<(), TaskError> {
        match lod {
            0   => Self::try_finish_full_mesh_task(full_tasks, pos, chunk, facade).await,
            lod => Self::try_finish_low_mesh_task(low_tasks, pos, lod, chunk, facade).await,
        }
    }

    pub async fn try_finish_full_mesh_task(
        full_tasks: &mut HashMap<Int3, FullTask>,
        pos: Int3, chunk: &mut Chunk, facade: &dyn gl::backend::Facade,
    ) -> Result<(), TaskError> {
        match full_tasks.get_mut(&pos) {
            Some(task) => match task.try_take_result().await {
                Some(vertices) => {
                    chunk.upload_full_detail_vertices(&vertices, facade);
                    let _ = full_tasks.remove(&pos)
                        .expect("there should be a task");
                    Ok(())
                },
                None => Err(TaskError::TaskNotReady),
            },
            None => Err(TaskError::TaskNotFound { lod: 0, pos }),
        }
    }
    
    pub async fn try_finish_low_mesh_task(
        low_tasks: &mut HashMap<(Int3, Lod), LowTask>,
        pos: Int3, lod: Lod,
        chunk: &mut Chunk, facade: &dyn gl::backend::Facade,
    ) -> Result<(), TaskError> {
        match low_tasks.get_mut(&(pos, lod)) {
            Some(task) => match task.try_take_result().await {
                Some(vertices) => {
                    chunk.upload_low_detail_vertices(&vertices, lod, facade);
                    let _ = low_tasks.remove(&(pos, lod))
                        .expect("there should be a task");
                    Ok(())
                },
                None => Err(TaskError::TaskNotReady),
            },
            None => Err(TaskError::TaskNotFound { lod, pos })
        }
    }

    pub fn can_start_tasks(&self) -> bool {
        self.saving_handle.is_none() && self.reading_handle.is_none() &&
        self.low_tasks.len() + self.full_tasks.len() <= cfg::terrain::MAX_TASKS
    }

    pub fn drop_tasks(&mut self) {
        drop(mem::take(&mut self.full_tasks));
        drop(mem::take(&mut self.low_tasks));
        drop(mem::take(&mut self.voxels_gen_tasks));
    }

    pub fn any_task_running(&self) -> bool {
        !self.low_tasks.is_empty() ||
        !self.full_tasks.is_empty() ||
        !self.voxels_gen_tasks.is_empty()
    }

    pub fn spawn_control_window(&mut self, ui: &imgui::Ui) {
        use crate::app::utils::graphics::ui::imgui_constructor::make_window;

        make_window(ui, "Chunk array")
            .always_auto_resize(true)
            .build(|| {
                ui.text(format!(
                    "{n} chunk generation tasks.",
                    n = self.voxels_gen_tasks.len()
                ));

                ui.text(format!(
                    "{n} mesh generation tasks.",
                    n = self.low_tasks.len() + self.full_tasks.len()
                ));

                ui.slider(
                    "Chunks lod threashold",
                    0.01, 20.0,
                    &mut self.lod_dist_threashold,
                );

                ui.separator();

                ui.text("Generate new");

                static SIZES: Mutex<[i32; 3]> = Mutex::new(Int3::ZERO.as_array());
                let mut sizes = SIZES.lock()
                    .expect("mutex should be not poisoned");

                ui.input_int3("Sizes", &mut *sizes)
                    .build();

                if ui.button("Generate") {
                    let sizes = USize3::from(Int3::from(*sizes).abs());

                    self.drop_tasks();
                    *self = Self::new_empty_chunks(sizes);
                }
            });
    }

    pub fn process_commands(&mut self, facade: &dyn Facade) {
        use crate::app::utils::terrain::chunk::commands::*;

        let mut commands = COMMAND_CHANNEL  
            .lock()
            .expect("mutex should be not poisoned");

        let mut change_tracker = ChangeTracker::new(self.sizes);

        while let Ok(command) = commands.receiver.try_recv() {
            match command {
                Command::SetVoxel { pos, new_id } => {
                    let old_id = self.set_voxel(pos, new_id)
                        .unwrap_or_else(|err| {
                            logger::log!(Error, "chunk array", format!("failed to set voxel: {err}"));
                            return 0;
                        });

                    if old_id != new_id {
                        change_tracker.track_voxel(pos);
                    }
                },

                Command::FillVoxels { pos_from, pos_to, new_id } => {
                    let _is_changed = self.fill_voxels(pos_from, pos_to, new_id)
                        .unwrap_or_else(|err| {
                            logger::log!(Error, "chunk array", format!("failed to fill voxels: {err}"));
                            return false;
                        });
                }

                Command::DropAllMeshes => self.drop_all_meshes(),
            }
        }

        let idxs_to_reload = change_tracker.idxs_to_reload();
        let n_changed = idxs_to_reload.len();
        self.reload_chunks_by_indices(idxs_to_reload, facade);

        if n_changed != 0 {
            logger::log!(Info, "chunk array", format!("{n_changed} chunks were updated!"));
        }
    }

    pub fn reload_chunks_by_indices(&mut self, indices: impl IntoIterator<Item = usize>, facade: &dyn Facade) {
        for idx in indices.into_iter() {
            let chunk_pos = Self::idx_to_pos(idx, self.sizes);

            // * It is safe, because current chunk mutable reference is not aliasing
            // * adjacent chunks shared references.
            let adj = unsafe { self.get_adj_chunks(chunk_pos).as_static() };

            match self.chunks.get_mut(idx) {
                Some(chunk) => chunk.generate_mesh(0, adj, facade),
                None => continue,
            }
        }
    }

    pub fn trace_ray(&self, ray: Line, max_steps: usize) -> impl Iterator<Item = Voxel> + '_ {
        (0..max_steps)
            .filter_map(move |i| {
                let pos = ray.point_along(i as f32 * 0.125);
                let pos = Int3::new(
                    pos.x.round() as i32,
                    pos.y.round() as i32,
                    pos.z.round() as i32,
                );

                self.get_voxel(pos)
            })
    }

    pub fn proccess_camera_input(&mut self, cam: &Camera) {
        const MAX_STEPS: usize = 1024;

        let first_voxel = self.trace_ray(Line::new(cam.pos, cam.front), MAX_STEPS)
            .filter(|voxel| !voxel.is_air())
            .next();

        match first_voxel {
            Some(voxel) => {
                use {
                    commands::{command, Command},
                    crate::app::utils::terrain::voxel::voxel_data::AIR_VOXEL_DATA
                };

                if mouse::just_left_pressed() {
                    command(Command::SetVoxel { pos: voxel.pos, new_id: AIR_VOXEL_DATA.id })
                }
            },

            None => (),
        }
    }

    pub async fn update(&mut self, facade: &dyn Facade, cam: &Camera) -> Result<(), UpdateError> {
        self.proccess_camera_input(cam);
        self.process_commands(facade);

        if keyboard::just_pressed_combo([Key::LControl, Key::S]) {
            let handle = tokio::spawn(
                ChunkArray::save_to_file(self.sizes, self.make_static_refs(), "world", "world")
            );
            self.saving_handle = Some(handle);
        }

        if self.saving_handle.is_some() && self.saving_handle.as_ref().unwrap().is_finished() {
            let handle = self.saving_handle.take().unwrap();
            handle.await??;
        }

        if keyboard::just_pressed_combo([Key::LControl, Key::O]) {
            let handle = tokio::spawn(ChunkArray::read_from_file("world", "world"));
            self.reading_handle = Some(handle);
        }

        if self.reading_handle.is_some() && self.reading_handle.as_ref().unwrap().is_finished() {
            let handle = self.reading_handle.take().unwrap();
            let (sizes, arr) = handle.await??;
            self.apply_new(sizes, arr);
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("failed to join task: {0}")]
    Join(#[from] JoinError),

    #[error("failed to save chunk array: {0}")]
    Save(#[from] io::Error),
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task is not already finished")]
    TaskNotReady,

    #[error("there is no task to generate mesh with lod {lod} and pos {pos} in map")]
    TaskNotFound {
        lod: Lod,
        pos: Int3,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChangeTracker {
    pub sizes: USize3,
    pub voxel_poses: HashSet<Int3>,
}

impl ChangeTracker {
    pub fn new(sizes: USize3) -> Self {
        Self { sizes, voxel_poses: HashSet::new() }
    }

    pub fn track_voxel(&mut self, voxel_pos: Int3) {
        self.voxel_poses.insert(voxel_pos);
    }

    pub fn idxs_to_reload(&self) -> HashSet<usize> {
        let mut result = HashSet::new();

        for &voxel_pos in self.voxel_poses.iter() {
            let chunk_pos = Chunk::local_pos(voxel_pos);
            let local_pos = Chunk::global_to_local_pos(chunk_pos, voxel_pos);

            let chunk_idx = match ChunkArray::pos_to_idx(self.sizes, chunk_pos) {
                Some(idx) => idx,
                None => continue,
            };

            for offset in iterator::offsets_from_border(local_pos, Int3::ZERO..Int3::from(Chunk::SIZES)) {
                match ChunkArray::pos_to_idx(self.sizes, chunk_pos + offset) {
                    Some(idx) => { result.insert(idx); },
                    None => continue,
                }
            }

            result.insert(chunk_idx);
        }

        result
    }
}