pub(super) struct Template<'a>(pub(super) &'a [u8]);

pub(super) struct Groups<'a> {
    pub(super) slots: Vec<Option<&'a [u8]>>,
    pub(super) names: &'a [Option<String>],
}

#[derive(Clone, Copy)]
enum Cap<'a> {
    Number(usize),
    Name(&'a str),
}

impl Template<'_> {
    pub(super) fn expand(&self, groups: &Groups<'_>, dst: &mut Vec<u8>) {
        let mut rest = self.0;
        while let Some(i) = rest.iter().position(|&b| b == b'$') {
            dst.extend_from_slice(&rest[..i]);
            rest = &rest[i..];
            if rest.get(1) == Some(&b'$') {
                dst.push(b'$');
                rest = &rest[2..];
                continue;
            }
            let Some((end, cap)) = Cap::parse(rest) else {
                dst.push(b'$');
                rest = &rest[1..];
                continue;
            };
            if let Some(bytes) = groups.get(cap) {
                dst.extend_from_slice(bytes);
            }
            rest = &rest[end..];
        }
        dst.extend_from_slice(rest);
    }
}

impl Groups<'_> {
    fn get(&self, cap: Cap<'_>) -> Option<&[u8]> {
        let i = match cap {
            Cap::Number(i) => i,
            Cap::Name(name) => self.names.iter().position(|n| n.as_deref() == Some(name))?,
        };
        self.slots.get(i).copied().flatten()
    }
}

impl<'a> Cap<'a> {
    fn parse(template: &'a [u8]) -> Option<(usize, Self)> {
        if template.len() <= 1 || template[0] != b'$' {
            return None;
        }
        let mut i = 1;
        let brace = template.get(1) == Some(&b'{');
        if brace {
            i = 2;
        }
        let mut end = i;
        while template.get(end).is_some_and(|&b| Self::letter(b)) {
            end += 1;
        }
        if end == i {
            return None;
        }
        let name = std::str::from_utf8(&template[i..end]).ok()?;
        if brace {
            if template.get(end) != Some(&b'}') {
                return None;
            }
            end += 1;
        }
        let cap = name
            .parse::<u32>()
            .map_or(Cap::Name(name), |n| Cap::Number(n as usize));
        Some((end, cap))
    }

    const fn letter(b: u8) -> bool {
        matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_')
    }
}
