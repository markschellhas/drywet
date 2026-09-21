mod package {
    #[test]
    fn version_is_semver() {
        assert_eq!(drywet::VERSION, "0.1.0");
    }
}
