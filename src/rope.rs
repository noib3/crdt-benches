use std::ops::Range;

use crdt_testdata::TestPatch;
use yrs::{updates::decoder::Decode, ReadTxn, Transact, Update};

pub trait Upstream {
    const NAME: &'static str;
    const EDITS_USE_BYTE_OFFSETS: bool = false;

    fn from_str(s: &str) -> Self;

    fn insert(&mut self, at_offset: usize, text: &str);

    fn remove(&mut self, between_offsets: Range<usize>);

    /// The returned length is interpreted as either number of codepoints or
    /// the number of bytes depending on the value of
    /// [`EDITS_USE_BYTE_OFFSETS`](Self::EDITS_USE_BYTE_OFFSETS).
    fn len(&self) -> usize;

    #[inline(always)]
    fn replace(&mut self, between_offsets: Range<usize>, text: &str) {
        let Range { start, end } = between_offsets;

        if end > start {
            self.remove(start..end);
        }

        if !text.is_empty() {
            self.insert(start, text);
        }
    }
}

#[derive(Clone)]
pub struct Automerge {
    doc: automerge::AutoCommit,
    text: Text,
}

#[derive(Debug, Clone, autosurgeon::Reconcile, autosurgeon::Hydrate)]
struct Text {
    text: autosurgeon::Text,
}

impl Upstream for Automerge {
    const NAME: &'static str = "automerge";

    #[inline(always)]
    fn from_str(s: &str) -> Self {
        let mut doc = automerge::AutoCommit::new();
        let text = self::Text { text: s.into() };
        autosurgeon::reconcile(&mut doc, &text).unwrap();
        Self { doc, text }
    }

    #[inline(always)]
    fn insert(&mut self, _: usize, _: &str) {
        unimplemented!()
    }

    #[inline(always)]
    fn remove(&mut self, _: Range<usize>) {
        unimplemented!()
    }

    #[inline(always)]
    fn replace(&mut self, range: Range<usize>, text: &str) {
        let len = range.end - range.start;
        self.text.text.splice(range.start, len as isize, text);
        autosurgeon::reconcile(&mut self.doc, &self.text).unwrap();
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.text.text.as_str().len()
    }
}

impl Upstream for cola::Replica {
    const NAME: &'static str = "cola";
    const EDITS_USE_BYTE_OFFSETS: bool = true;

    #[inline(always)]
    fn from_str(s: &str) -> Self {
        Self::new(1, s.len())
    }

    #[inline(always)]
    fn insert(&mut self, at: usize, s: &str) {
        let _ = self.inserted(at, s.len());
    }

    #[inline(always)]
    fn remove(&mut self, range: Range<usize>) {
        let _ = self.deleted(range);
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.len()
    }
}

#[derive(Clone)]
pub struct Dt {
    oplog: diamond_types::list::OpLog,
    agent: u32,
    time: usize,
}

impl Upstream for Dt {
    const NAME: &'static str = "diamond-types";

    #[inline(always)]
    fn from_str(s: &str) -> Self {
        let mut oplog = diamond_types::list::OpLog::new();
        let agent = oplog.get_or_create_agent_id("bench");
        let time = oplog.add_insert(agent, 0, s);
        Self { oplog, agent, time }
    }

    #[inline(always)]
    fn insert(&mut self, at: usize, s: &str) {
        self.time = self.oplog.add_insert(self.agent, at, s);
    }

    #[inline(always)]
    fn remove(&mut self, range: Range<usize>) {
        self.time = self.oplog.add_delete_without_content(self.agent, range);
    }

    #[inline(always)]
    fn len(&self) -> usize {
        self.oplog.checkout_tip().len()
    }
}

#[derive(Clone)]
pub struct Yrs {
    doc: yrs::Doc,
    text: yrs::types::text::TextRef,
}

impl Upstream for Yrs {
    const NAME: &'static str = "yrs";
    const EDITS_USE_BYTE_OFFSETS: bool = true;

    #[inline(always)]
    fn from_str(s: &str) -> Self {
        use yrs::Text;
        let doc = yrs::Doc::new();
        let text = doc.get_or_insert_text("bench");
        {
            let mut txn = doc.transact_mut();
            text.push(&mut txn, s);
        }
        Self { doc, text }
    }

    #[inline(always)]
    fn insert(&mut self, at: usize, s: &str) {
        use yrs::Text;
        let mut txn = self.doc.transact_mut();
        self.text.insert(&mut txn, at as u32, s);
    }

    #[inline(always)]
    fn remove(&mut self, range: Range<usize>) {
        use yrs::Text;
        let len = range.end - range.start;
        let mut txn = self.doc.transact_mut();
        self.text
            .remove_range(&mut txn, range.start as u32, len as u32);
    }

    #[inline(always)]
    fn len(&self) -> usize {
        use yrs::Text;
        let txn = self.doc.transact();
        self.text.len(&txn) as usize
    }
}

pub trait Downstream: Upstream + Clone {
    type Update;

    fn upstream_updates(trace: &crdt_testdata::TestData) -> (Self, Vec<Self::Update>);

    fn apply_update(&mut self, update: &Self::Update);
}

impl Downstream for Dt {
    type Update = Vec<u8>;

    fn upstream_updates(trace: &crdt_testdata::TestData) -> (Self, Vec<Self::Update>) {
        let mut upstream = Self::from_str(&trace.start_content);

        let mut updates = Vec::new();

        let options = diamond_types::list::encoding::EncodeOptions {
            user_data: None,
            store_start_branch_content: true,
            store_inserted_content: false,
            store_deleted_content: false,
            compress_content: false,
            verbose: false,
        };

        for txn in &trace.txns {
            for TestPatch(pos, del, ins) in &txn.patches {
                let encode_from = upstream.time;
                upstream.replace(*pos..*pos + del, ins);
                let update = upstream.oplog.encode_from(options.clone(), &[encode_from]);
                updates.push(update);
            }
        }

        (Self::from_str(&trace.start_content), updates)
    }

    fn apply_update(&mut self, update: &Vec<u8>) {
        let _ = self.oplog.decode_and_add(update.as_slice());
    }
}

impl Downstream for Automerge {
    type Update = Self;

    fn upstream_updates(trace: &crdt_testdata::TestData) -> (Self, Vec<Self::Update>) {
        todo!();
    }

    fn apply_update(&mut self, other: &Self::Update) {
        let _ = self.doc.merge(&mut (other.doc.clone()));
    }
}

impl Downstream for Yrs {
    type Update = yrs::Update;

    fn upstream_updates(trace: &crdt_testdata::TestData) -> (Self, Vec<Self::Update>) {
        let mut upstream = Self::from_str(&trace.start_content);

        let downstream = Self::from_str(&trace.start_content);

        let mut updates = Vec::new();

        for txn in &trace.txns {
            for TestPatch(pos, del, ins) in &txn.patches {
                upstream.replace(*pos..*pos + del, ins);
                let sv = downstream.doc.transact().state_vector();
                let enc_update = upstream.doc.transact().encode_diff_v1(&sv);
                let update = Update::decode_v1(&enc_update).unwrap();
                downstream.doc.transact_mut().apply_update(update);
                let update = Update::decode_v1(&enc_update).unwrap();
                updates.push(update);
            }
        }

        (Self::from_str(&trace.start_content), updates)
    }

    #[inline(always)]
    fn apply_update(&mut self, update: &Self::Update) {
        todo!();
        // self.doc.transact_mut().apply_update(update);
    }
}
