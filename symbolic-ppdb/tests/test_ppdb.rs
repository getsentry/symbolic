use symbolic_ppdb::PortablePdb;
use symbolic_testutils::fixture;

#[test]
fn test_embedded_sources_missing() {
    let buf = std::fs::read(fixture("windows/portable.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let mut iter = ppdb.get_embedded_sources().unwrap();
    assert!(iter.next().is_none());
}

#[test]
fn test_embedded_sources() {
    let buf = std::fs::read(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 4);

    let check_path = |i: usize, expected: &str| {
        let repo_root = "C:\\dev\\sentry-dotnet\\samples\\Sentry.Samples.Console.Basic\\";
        assert_eq!(items[i].get_path(), format!("{}{}", repo_root, expected));
    };

    check_path(0, "Program.cs");
    check_path(
        1,
        "obj\\release\\net6.0\\Sentry.Samples.Console.Basic.GlobalUsings.g.cs",
    );
    check_path(
        2,
        "obj\\release\\net6.0\\.NETCoreApp,Version=v6.0.AssemblyAttributes.cs",
    );
    check_path(
        3,
        "obj\\release\\net6.0\\Sentry.Samples.Console.Basic.AssemblyInfo.cs",
    );
}

#[test]
fn test_embedded_sources_contents() {
    let buf = std::fs::read(fixture("windows/Sentry.Samples.Console.Basic.pdb")).unwrap();

    let ppdb = PortablePdb::parse(&buf).unwrap();
    let iter = ppdb.get_embedded_sources().unwrap();
    let items = iter.collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(items.len(), 4);

    let check_contents = |i: usize, length: usize, name: &str| {
        let content = items[i].get_contents().unwrap();
        assert_eq!(content.len(), length);

        let expected = std::fs::read(format!("tests/fixtures/contents/{}", name)).unwrap();
        assert_eq!(content, expected);
    };

    check_contents(0, 204, "Program.cs");
    check_contents(1, 295, "Sentry.Samples.Console.Basic.GlobalUsings.g.cs");
    check_contents(2, 198, ".NETCoreApp,Version=v6.0.AssemblyAttributes.cs");
    check_contents(3, 1019, "Sentry.Samples.Console.Basic.AssemblyInfo.cs");
}
